use cosmwasm_std::{
    entry_point, to_json_binary, Addr, BankMsg, Binary, Coin, Deps, DepsMut, Env,
    MessageInfo, Order, QuerierWrapper, Response, StdResult, Uint128,
};
use cw2::set_contract_version;

use crate::custom::{burn_tokens_msg, create_denom_msg, mint_and_send_tokens_msg};
use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, PendingWithdrawalsResponse, QueryMsg,
    UserInfoResponse, VaultInfoResponse, WithdrawalInfo, WithdrawalResponse,
};
use crate::state::{
    Config, PendingWithdrawal, VaultState, WithdrawalCounter, CONFIG, CONTRACT_NAME,
    CONTRACT_VERSION, MAX_WITHDRAWAL_DELAY, MIN_WITHDRAWAL_DELAY, PENDING_WITHDRAWALS,
    VAULT_STATE, WITHDRAWAL_COUNTERS,
};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    // Validate subdenom requirements (3-44 chars, start with lowercase)
    if msg.lp_subdenom.len() < 3 || msg.lp_subdenom.len() > 44 {
        return Err(ContractError::InvalidSubdenom {
            reason: "Subdenom must be 3-44 characters".to_string(),
        });
    }

    if !msg
        .lp_subdenom
        .chars()
        .next()
        .unwrap()
        .is_ascii_lowercase()
    {
        return Err(ContractError::InvalidSubdenom {
            reason: "Subdenom must start with lowercase letter".to_string(),
        });
    }

    // Validate minting cap (must be > 0 as per ZigChain requirements)
    if msg.lp_minting_cap.is_zero() {
        return Err(ContractError::InvalidMintingCap {});
    }

    // Validate withdrawal delay - REQUIRED and IMMUTABLE after instantiation
    // Security check: withdrawal delay must be within reasonable bounds
    // Min: 2 minutes (120s) for flexibility, Max: 30 days (2,592,000s) for security
    let withdrawal_delay = msg.withdrawal_delay_seconds;
    if !(MIN_WITHDRAWAL_DELAY..=MAX_WITHDRAWAL_DELAY).contains(&withdrawal_delay) {
        return Err(ContractError::InvalidWithdrawalDelay {
            min: MIN_WITHDRAWAL_DELAY,
            max: MAX_WITHDRAWAL_DELAY,
        });
    }

    let admin_addr = match msg.admin {
        Some(addr) => deps.api.addr_validate(&addr)?,
        None => info.sender.clone(),
    };

    // Construct LP token full denom
    let lp_full_denom = format!("coin.{}.{}", env.contract.address, msg.lp_subdenom);

    // Initialize configuration with IMMUTABLE withdrawal_delay
    // No yield contract address - admin manages funds manually
    let config = Config {
        stablecoin_denom: msg.stablecoin_denom.clone(),
        lp_full_denom: lp_full_denom.clone(),
        admin: admin_addr.clone(),
        withdrawal_delay,
    };
    CONFIG.save(deps.storage, &config)?;

    // Initialize vault state - starts empty
    // Admin will manage deposits to yield protocols manually
    let vault_state = VaultState {
        total_deposited: Uint128::zero(),
        total_lp_minted: Uint128::zero(),
        total_pending_withdrawals: Uint128::zero(),
    };
    VAULT_STATE.save(deps.storage, &vault_state)?;

    // Set contract version
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // Create LP token denom via TokenFactory
    let create_denom = create_denom_msg(
        env.contract.address.to_string(),
        msg.lp_subdenom.clone(),
        msg.lp_minting_cap.to_string(),
        msg.can_change_minting_cap.unwrap_or(true),
        msg.uri,
        msg.uri_hash,
        msg.description,
    )?;

    Ok(Response::new()
        .add_message(create_denom)
        .add_attribute("method", "instantiate")
        .add_attribute("stablecoin_denom", msg.stablecoin_denom)
        .add_attribute("lp_full_denom", lp_full_denom)
        .add_attribute("lp_minting_cap", msg.lp_minting_cap)
        .add_attribute("withdrawal_delay", withdrawal_delay.to_string())
        .add_attribute("admin", admin_addr))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Deposit {} => execute_deposit(deps, env, info),
        ExecuteMsg::RequestWithdraw {} => execute_request_withdraw(deps, env, info),
        ExecuteMsg::ClaimWithdraw { withdrawal_id } => {
            execute_claim_withdraw(deps, env, info, withdrawal_id)
        }
        ExecuteMsg::UpdateConfig {
            stablecoin_denom,
            admin,
        } => execute_update_config(deps, info, stablecoin_denom, admin),
        ExecuteMsg::AdminWithdraw { amount } => {
            execute_admin_withdraw(deps, env, info, amount)
        }
        ExecuteMsg::AdminDepositYield {
            principal_amount,
            yield_amount,
        } => execute_admin_deposit_yield(deps, env, info, principal_amount, yield_amount),
    }
}

fn execute_deposit(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Security: Ensure only stablecoin is sent, nothing else
    if info.funds.len() != 1 {
        return Err(ContractError::Std(cosmwasm_std::StdError::generic_err(
            "Must send exactly one token type",
        )));
    }

    // Find and validate the stablecoin sent
    let stablecoin_sent = info
        .funds
        .iter()
        .find(|coin| coin.denom == config.stablecoin_denom)
        .ok_or(ContractError::NoStablecoinSent {})?;

    let deposit_amount = stablecoin_sent.amount;
    if deposit_amount.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    // Load current vault state
    let mut vault_state = VAULT_STATE.load(deps.storage)?;

    // Calculate how many LP tokens to mint to the depositor
    // Using the vault share pricing model from PRICECAL.MD:
    // sharesToMint = depositAmount / pricePerShare
    // where pricePerShare = totalAssets / totalShares
    let lp_to_mint = if vault_state.total_lp_minted.is_zero() || vault_state.total_deposited.is_zero() {
        // First deposit or vault value is zero: mint 1:1
        deposit_amount
    } else {
        // Calculate LP tokens: deposit_amount * total_lp_minted / total_deposited
        deposit_amount
            .checked_multiply_ratio(vault_state.total_lp_minted, vault_state.total_deposited)
            .map_err(|e| cosmwasm_std::StdError::generic_err(format!("Multiply ratio error: {}", e)))?
    };

    if lp_to_mint.is_zero() {
        return Err(ContractError::Std(cosmwasm_std::StdError::generic_err(
            "Deposit too small - would mint zero LP tokens",
        )));
    }

    // Update vault state
    vault_state.total_deposited = vault_state
        .total_deposited
        .checked_add(deposit_amount)
        .map_err(|_| ContractError::OverflowError {
            operation: "total deposited".to_string(),
        })?;

    vault_state.total_lp_minted = vault_state
        .total_lp_minted
        .checked_add(lp_to_mint)
        .map_err(|_| ContractError::OverflowError {
            operation: "LP mint".to_string(),
        })?;

    // Save state before external calls
    VAULT_STATE.save(deps.storage, &vault_state)?;

    // Mint LP tokens to the user via TokenFactory
    let mint_msg = mint_and_send_tokens_msg(
        env.contract.address.to_string(),
        config.lp_full_denom.clone(),
        lp_to_mint.to_string(),
        info.sender.to_string(),
    )?;

    Ok(Response::new()
        .add_message(mint_msg)
        .add_attribute("method", "deposit")
        .add_attribute("user", info.sender)
        .add_attribute("stablecoin_amount", deposit_amount)
        .add_attribute("lp_minted", lp_to_mint)
        .add_attribute("vault_value", vault_state.total_deposited))
}

fn execute_request_withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Security: Ensure only LP tokens are sent, nothing else
    if info.funds.len() != 1 {
        return Err(ContractError::Std(cosmwasm_std::StdError::generic_err(
            "Must send exactly one token type",
        )));
    }

    // Find and validate the LP tokens sent
    let lp_sent = info
        .funds
        .iter()
        .find(|coin| coin.denom == config.lp_full_denom)
        .ok_or(ContractError::NoLpTokensSent {})?;

    let lp_amount = lp_sent.amount;
    if lp_amount.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    // Load vault state for validation
    let mut vault_state = VAULT_STATE.load(deps.storage)?;

    // Security check: Ensure LP tokens being burned don't exceed total minted
    if vault_state.total_lp_minted < lp_amount {
        return Err(ContractError::Std(cosmwasm_std::StdError::generic_err(
            "LP amount exceeds total minted supply",
        )));
    }

    // Calculate stablecoin amount to return to user based on their LP token share
    // Using PRICECAL.MD formula: returnAmount = shares × pricePerShare
    // where pricePerShare = totalAssets / totalShares
    let stablecoin_amount = if vault_state.total_lp_minted.is_zero() {
        return Err(ContractError::Std(cosmwasm_std::StdError::generic_err(
            "No LP tokens in circulation",
        )));
    } else {
        // User's share of vault = lp_amount * total_deposited / total_lp_minted
        lp_amount
            .checked_multiply_ratio(vault_state.total_deposited, vault_state.total_lp_minted)
            .map_err(|e| cosmwasm_std::StdError::generic_err(format!("Multiply ratio error: {}", e)))?
    };

    if stablecoin_amount.is_zero() {
        return Err(ContractError::Std(cosmwasm_std::StdError::generic_err(
            "Withdrawal amount too small",
        )));
    }

    // Update vault state - burn LP tokens from circulation
    vault_state.total_lp_minted = vault_state
        .total_lp_minted
        .checked_sub(lp_amount)
        .map_err(|_| ContractError::OverflowError {
            operation: "LP burn".to_string(),
        })?;

    // Add to pending withdrawals
    vault_state.total_pending_withdrawals = vault_state
        .total_pending_withdrawals
        .checked_add(stablecoin_amount)
        .map_err(|_| ContractError::OverflowError {
            operation: "pending withdrawal".to_string(),
        })?;

    // Save vault state before any external calls
    VAULT_STATE.save(deps.storage, &vault_state)?;

    // Get or initialize withdrawal counter for this user
    let user_addr = info.sender.clone();
    let mut counter = WITHDRAWAL_COUNTERS
        .may_load(deps.storage, &user_addr)?
        .unwrap_or(WithdrawalCounter { next_id: 0 });

    let withdrawal_id = counter.next_id;

    // Security check: Prevent withdrawal ID overflow
    counter.next_id = counter
        .next_id
        .checked_add(1)
        .ok_or(ContractError::OverflowError {
            operation: "withdrawal ID".to_string(),
        })?;

    WITHDRAWAL_COUNTERS.save(deps.storage, &user_addr, &counter)?;

    // Calculate release time using IMMUTABLE config.withdrawal_delay
    // Security: Use config value, not a constant that could be bypassed
    let release_time = env.block.time.plus_seconds(config.withdrawal_delay);

    // Create pending withdrawal
    let pending_withdrawal = PendingWithdrawal {
        amount: stablecoin_amount,
        release_time,
    };

    PENDING_WITHDRAWALS.save(deps.storage, (&user_addr, withdrawal_id), &pending_withdrawal)?;

    // Burn LP tokens via TokenFactory
    let burn_msg = burn_tokens_msg(
        env.contract.address.to_string(),
        config.lp_full_denom.clone(),
        lp_amount.to_string(),
    )?;

    Ok(Response::new()
        .add_message(burn_msg)
        .add_attribute("method", "request_withdraw")
        .add_attribute("user", user_addr)
        .add_attribute("withdrawal_id", withdrawal_id.to_string())
        .add_attribute("lp_burned", lp_amount)
        .add_attribute("stablecoin_amount_with_yield", stablecoin_amount)
        .add_attribute("release_time", release_time.to_string())
        .add_attribute("vault_total_deposited", vault_state.total_deposited))
}

fn execute_claim_withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    withdrawal_id: u64,
) -> Result<Response, ContractError> {
    // Security: No funds should be sent with claim
    if !info.funds.is_empty() {
        return Err(ContractError::Std(cosmwasm_std::StdError::generic_err(
            "Do not send funds when claiming withdrawal",
        )));
    }

    let config = CONFIG.load(deps.storage)?;
    let user_addr = info.sender.clone();

    // Security: Load and immediately remove to prevent reentrancy
    // This follows checks-effects-interactions pattern
    let pending_withdrawal = PENDING_WITHDRAWALS
        .may_load(deps.storage, (&user_addr, withdrawal_id))?
        .ok_or(ContractError::WithdrawalNotFound {})?;

    // Security check: Verify time lock has expired
    if env.block.time < pending_withdrawal.release_time {
        return Err(ContractError::WithdrawalLocked {
            release_time: pending_withdrawal.release_time.seconds(),
        });
    }

    let stablecoin_amount = pending_withdrawal.amount;

    // Security: Validate amount is not zero (should never happen but check anyway)
    if stablecoin_amount.is_zero() {
        return Err(ContractError::Std(cosmwasm_std::StdError::generic_err(
            "Invalid withdrawal amount",
        )));
    }

    // Load vault state for validation and update
    let mut vault_state = VAULT_STATE.load(deps.storage)?;

    // Security check: Ensure pending withdrawals accounting is correct
    if vault_state.total_pending_withdrawals < stablecoin_amount {
        return Err(ContractError::Std(cosmwasm_std::StdError::generic_err(
            "Pending withdrawals accounting mismatch",
        )));
    }

    // Check if vault has enough liquid stablecoin balance
    // Admin must ensure sufficient liquidity by bringing funds back from yield protocols
    let vault_balance = query_bank_balance(
        &deps.querier,
        &env.contract.address,
        &config.stablecoin_denom,
    )?;

    if vault_balance < stablecoin_amount {
        return Err(ContractError::InsufficientContractBalance {
            available: vault_balance,
            required: stablecoin_amount,
        });
    }

    // Update vault state BEFORE external calls (reentrancy protection)
    // Decrease both total_deposited and total_pending_withdrawals
    vault_state.total_deposited = vault_state
        .total_deposited
        .checked_sub(stablecoin_amount)
        .map_err(|_| ContractError::OverflowError {
            operation: "total deposited decrease".to_string(),
        })?;

    vault_state.total_pending_withdrawals = vault_state
        .total_pending_withdrawals
        .checked_sub(stablecoin_amount)
        .map_err(|_| ContractError::OverflowError {
            operation: "pending withdrawal decrease".to_string(),
        })?;

    // Save state before external call
    VAULT_STATE.save(deps.storage, &vault_state)?;

    // Remove pending withdrawal BEFORE external call (prevents double-claim)
    PENDING_WITHDRAWALS.remove(deps.storage, (&user_addr, withdrawal_id));

    // Transfer stablecoin to user via Bank module
    let return_msg = BankMsg::Send {
        to_address: user_addr.to_string(),
        amount: vec![Coin {
            denom: config.stablecoin_denom.clone(),
            amount: stablecoin_amount,
        }],
    };

    Ok(Response::new()
        .add_message(return_msg)
        .add_attribute("method", "claim_withdraw")
        .add_attribute("user", user_addr)
        .add_attribute("withdrawal_id", withdrawal_id.to_string())
        .add_attribute("stablecoin_amount_with_yield", stablecoin_amount)
        .add_attribute("vault_total_deposited", vault_state.total_deposited))
}

fn execute_update_config(
    deps: DepsMut,
    info: MessageInfo,
    stablecoin_denom: Option<String>,
    admin: Option<String>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    // Security: Only admin can update config
    if info.sender != config.admin {
        return Err(ContractError::Unauthorized {});
    }

    // Update stablecoin denom if provided
    if let Some(denom) = stablecoin_denom {
        config.stablecoin_denom = denom;
    }

    // Update admin if provided
    if let Some(addr) = admin {
        config.admin = deps.api.addr_validate(&addr)?;
    }

    // Security: withdrawal_delay is IMMUTABLE and cannot be changed
    // It remains set to the value from instantiation
    // This prevents any attempts to bypass time locks

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("method", "update_config")
        .add_attribute("admin", info.sender))
}

/// Admin-only function to move stablecoin from vault to external yield protocol
/// This allows admin to deposit funds into yield-generating contracts
/// Does not change vault accounting (funds still belong to vault)
/// Admin-only function to withdraw stablecoin from vault to admin's wallet
/// This allows admin to manage funds externally for yield generation
/// Admin is responsible for depositing funds + yield back later via AdminDepositYield
///
/// Security features:
/// - Admin-only access (only info.sender == config.admin)
/// - Withdraws to admin wallet only (not to arbitrary addresses)
/// - Checks vault has sufficient balance
/// - No funds should be sent with this call
/// - Does NOT decrease total_deposited (accounting unchanged until yield deposited back)
fn execute_admin_withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Security: Only admin can withdraw
    if info.sender != config.admin {
        return Err(ContractError::Unauthorized {});
    }

    // Security: No funds should be sent with this call
    if !info.funds.is_empty() {
        return Err(ContractError::Std(cosmwasm_std::StdError::generic_err(
            "Do not send funds with this call",
        )));
    }

    // Validate amount
    if amount.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    // Check vault has sufficient balance
    let vault_balance = query_bank_balance(
        &deps.querier,
        &env.contract.address,
        &config.stablecoin_denom,
    )?;

    if vault_balance < amount {
        return Err(ContractError::InsufficientContractBalance {
            available: vault_balance,
            required: amount,
        });
    }

    // Note: We do NOT decrease total_deposited here
    // The funds still belong to the vault (tracked in accounting)
    // Admin must deposit  funds + yield back via AdminDepositYield
    // This maintains the price per share until yield is reported

    // Transfer stablecoin to admin's wallet
    let transfer_msg = BankMsg::Send {
        to_address: info.sender.to_string(),
        amount: vec![Coin {
            denom: config.stablecoin_denom.clone(),
            amount,
        }],
    };

    Ok(Response::new()
        .add_message(transfer_msg)
        .add_attribute("method", "admin_withdraw")
        .add_attribute("admin", info.sender)
        .add_attribute("amount", amount))
}

/// Admin-only function to deposit yield directly into the vault
/// This replaces the old two-step process with a single atomic operation:
/// 1. Admin sends stablecoin (yield) via info.funds
/// 2. Contract automatically increases total_deposited
/// 3. No LP tokens are minted (only increases price per share)
/// 4. All LP holders benefit proportionally
///
/// Security features:
/// - Admin-only access
/// - Requires exact stablecoin denom
/// - No LP token minting
/// - Automatic accounting update
/// - Checked arithmetic prevents overflow
fn execute_admin_deposit_yield(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    principal_amount: Uint128,
    yield_amount: Uint128,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Security: Only admin can deposit yield
    if info.sender != config.admin {
        return Err(ContractError::Unauthorized {});
    }

    // Validate that exactly one token type is sent and it's the stablecoin
    if info.funds.len() != 1 {
        return Err(ContractError::Std(cosmwasm_std::StdError::generic_err(
            "Must send exactly one token type (stablecoin)",
        )));
    }

    let received_coin = info
        .funds
        .iter()
        .find(|coin| coin.denom == config.stablecoin_denom)
        .ok_or(ContractError::NoStablecoinSent {})?;

    let total_received = received_coin.amount;

    // Validate amounts
    let expected_total = principal_amount
        .checked_add(yield_amount)
        .map_err(|_| ContractError::OverflowError {
            operation: "principal + yield".to_string(),
        })?;

    if total_received != expected_total {
        return Err(ContractError::Std(cosmwasm_std::StdError::generic_err(
            format!(
                "Amount mismatch: sent {} but declared principal {} + yield {} = {}",
                total_received, principal_amount, yield_amount, expected_total
            ),
        )));
    }

    // Principal can be zero (pure yield deposit) but both cannot be zero
    if principal_amount.is_zero() && yield_amount.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    // Load current vault state
    let mut vault_state = VAULT_STATE.load(deps.storage)?;

    let old_total = vault_state.total_deposited;

    // CRITICAL: Only add yield_amount to total_deposited
    // Principal is just returning funds that admin withdrew earlier
    // Adding principal would double-count it (it's already in total_deposited from original deposit)
    vault_state.total_deposited = vault_state
        .total_deposited
        .checked_add(yield_amount)
        .map_err(|_| ContractError::OverflowError {
            operation: "adding yield to total_deposited".to_string(),
        })?;

    let new_total = vault_state.total_deposited;

    // Save updated state
    VAULT_STATE.save(deps.storage, &vault_state)?;

    // Calculate new price per share for logging
    let price_per_share = if vault_state.total_lp_minted.is_zero() {
        "1.0".to_string()
    } else {
        let price_ratio = new_total
            .checked_multiply_ratio(Uint128::new(1_000_000), vault_state.total_lp_minted)
            .map_err(|e| cosmwasm_std::StdError::generic_err(format!("Multiply ratio error: {}", e)))?;
        format!(
            "{}.{:06}",
            price_ratio / Uint128::new(1_000_000),
            price_ratio % Uint128::new(1_000_000)
        )
    };

    Ok(Response::new()
        .add_attribute("method", "admin_deposit_yield")
        .add_attribute("admin", info.sender)
        .add_attribute("principal_returned", principal_amount)
        .add_attribute("yield_deposited", yield_amount)
        .add_attribute("total_received", total_received)
        .add_attribute("old_total_deposited", old_total)
        .add_attribute("new_total_deposited", new_total)
        .add_attribute("price_per_share", price_per_share))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&query_config(deps)?),
        QueryMsg::VaultInfo {} => to_json_binary(&query_vault_info(deps, env.clone())?),
        QueryMsg::UserInfo { address } => to_json_binary(&query_user_info(deps, env.clone(), address)?),
        QueryMsg::PendingWithdrawals { address } => {
            to_json_binary(&query_pending_withdrawals(deps, env.clone(), address)?)
        }
        QueryMsg::Withdrawal {
            address,
            withdrawal_id,
        } => to_json_binary(&query_withdrawal(deps, env, address, withdrawal_id)?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        stablecoin_denom: config.stablecoin_denom,
        lp_full_denom: config.lp_full_denom,
        admin: config.admin,
        withdrawal_delay: config.withdrawal_delay,
    })
}

fn query_vault_info(deps: Deps, _env: Env) -> StdResult<VaultInfoResponse> {
    let vault_state = VAULT_STATE.load(deps.storage)?;

    // Calculate price per share
    // pricePerShare = totalAssets / totalShares
    let price_per_share = if vault_state.total_lp_minted.is_zero() {
        "1.0".to_string()
    } else {
        // Calculate as a decimal string: total_deposited / total_lp_minted
        // Multiply by 1_000_000 for 6 decimal places precision
        let price_ratio = vault_state
            .total_deposited
            .checked_multiply_ratio(Uint128::new(1_000_000), vault_state.total_lp_minted)
            .map_err(|e| cosmwasm_std::StdError::generic_err(format!("Multiply ratio error: {}", e)))?;
        format!(
            "{}.{:06}",
            price_ratio / Uint128::new(1_000_000),
            price_ratio % Uint128::new(1_000_000)
        )
    };

    Ok(VaultInfoResponse {
        total_deposited: vault_state.total_deposited,
        total_lp_supply: vault_state.total_lp_minted,
        total_pending_withdrawals: vault_state.total_pending_withdrawals,
        price_per_share,
    })
}

fn query_user_info(deps: Deps, _env: Env, address: String) -> StdResult<UserInfoResponse> {
    let config = CONFIG.load(deps.storage)?;
    let vault_state = VAULT_STATE.load(deps.storage)?;
    let user_addr = deps.api.addr_validate(&address)?;

    // Query LP balance from bank module
    let lp_balance = query_bank_balance(&deps.querier, &user_addr, &config.lp_full_denom)?;

    // Calculate user's share of the vault's value (including yield)
    // stablecoin_value = user_lp_balance * total_deposited / total_lp_supply
    let stablecoin_value = if vault_state.total_lp_minted.is_zero() {
        Uint128::zero()
    } else {
        lp_balance
            .checked_multiply_ratio(vault_state.total_deposited, vault_state.total_lp_minted)
            .map_err(|e| cosmwasm_std::StdError::generic_err(format!("Multiply ratio error: {}", e)))?
    };

    Ok(UserInfoResponse {
        address: user_addr,
        lp_balance,
        stablecoin_value,
    })
}

fn query_pending_withdrawals(
    deps: Deps,
    env: Env,
    address: String,
) -> StdResult<PendingWithdrawalsResponse> {
    let user_addr = deps.api.addr_validate(&address)?;

    let withdrawals: Vec<WithdrawalInfo> = PENDING_WITHDRAWALS
        .prefix(&user_addr)
        .range(deps.storage, None, None, Order::Ascending)
        .map(|item| {
            let (id, withdrawal) = item?;
            let claimable = env.block.time >= withdrawal.release_time;
            Ok(WithdrawalInfo {
                id,
                amount: withdrawal.amount,
                release_time: withdrawal.release_time,
                claimable,
            })
        })
        .collect::<StdResult<Vec<WithdrawalInfo>>>()?;

    Ok(PendingWithdrawalsResponse {
        address: user_addr,
        withdrawals,
    })
}

fn query_withdrawal(
    deps: Deps,
    env: Env,
    address: String,
    withdrawal_id: u64,
) -> StdResult<WithdrawalResponse> {
    let user_addr = deps.api.addr_validate(&address)?;

    let withdrawal = PENDING_WITHDRAWALS
        .may_load(deps.storage, (&user_addr, withdrawal_id))?
        .ok_or_else(|| cosmwasm_std::StdError::generic_err("Withdrawal not found"))?;

    let claimable = env.block.time >= withdrawal.release_time;

    Ok(WithdrawalResponse {
        address: user_addr,
        withdrawal: WithdrawalInfo {
            id: withdrawal_id,
            amount: withdrawal.amount,
            release_time: withdrawal.release_time,
            claimable,
        },
    })
}

/// Query balance from bank module
fn query_bank_balance(
    querier: &QuerierWrapper,
    address: &Addr,
    denom: &str,
) -> StdResult<Uint128> {
    let balance = querier.query_balance(address, denom)?;
    Ok(balance.amount)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}
