#[cfg(feature = "test-addr-bypass")]
use defa_types::addr_validate_bypass::addr_validate_bypass;
use cosmwasm_std::{
    entry_point, to_json_binary, Addr, BankMsg, Binary, Coin, Deps, DepsMut, Env,
    MessageInfo, Order, QuerierWrapper, Response, StdResult, Uint128,
};
use cw2::set_contract_version;

use crate::custom::{burn_tokens_msg, create_denom_msg, mint_and_send_tokens_msg};
use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, LpHoldersResponse, MigrateMsg,
    OldestQueueWaitDaysResponse, PendingWithdrawalsResponse, QueryMsg,
    PoolStateResponse, UserInfoResponse, VaultInfoResponse, WithdrawalInfo,
    WithdrawalResponse,
};
use defa_yield_reserve::msg as yield_reserve_msg;
use crate::state::{
    day_of_week, Config, PendingWithdrawal, QueueEntry, VaultState,
    WithdrawalCounter, CONFIG, CONTRACT_NAME, CONTRACT_VERSION,
    LP_HOLDERS, MAX_WITHDRAWAL_DELAY, MIN_WITHDRAWAL_DELAY,
    MONDAY_WITHDRAWAL_WINDOW_DAY, PENDING_WITHDRAWALS, PAUSED, POOL_STATE,
    USER_WITHDRAWAL_QUEUE_ID, VAULT_STATE, WITHDRAWAL_COUNTERS,
    WITHDRAWAL_QUEUE, WITHDRAWAL_QUEUE_HEAD, WITHDRAWAL_QUEUE_NEXT,
};
use defa_types::{PoolState, SECONDS_PER_DAY};

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
        .ok_or(ContractError::InvalidSubdenom {
            reason: "subdenom must not be empty".to_string(),
        })?
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
        Some(addr) => {
            #[cfg(feature = "test-addr-bypass")]
            { cosmwasm_std::Addr::unchecked(addr_validate_bypass(&addr).map_err(|e| cosmwasm_std::StdError::generic_err(e))?) }
            #[cfg(not(feature = "test-addr-bypass"))]
            { deps.api.addr_validate(&addr)? }
        },
        None => info.sender.clone(),
    };

    // Construct LP token full denom
    let lp_full_denom = format!("coin.{}.{}", env.contract.address, msg.lp_subdenom);

    // Parse yield_reserve address
    let yield_reserve_addr = deps.api.addr_validate(&msg.yield_reserve)?;

    // Initialize configuration with IMMUTABLE withdrawal_delay
    let config = Config {
        stablecoin_denom: msg.stablecoin_denom.clone(),
        lp_full_denom: lp_full_denom.clone(),
        admin: admin_addr.clone(),
        withdrawal_delay,
        credit_manager: None,
        investor_apy_bps: msg.investor_apy_bps,
        yield_reserve: yield_reserve_addr,
        execution_amount: msg.lp_minting_cap, // Use minting cap as execution amount for now
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
    POOL_STATE.save(deps.storage, &PoolState::Fundraising)?;
    PAUSED.save(deps.storage, &false)?;
    WITHDRAWAL_QUEUE_HEAD.save(deps.storage, &0u64)?;
    WITHDRAWAL_QUEUE_NEXT.save(deps.storage, &0u64)?;

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
    // PAUSE GUARD — covers all 16 execute handlers. Update this count when adding new handlers.
    let paused = PAUSED.may_load(deps.storage)?.unwrap_or(false);
    if paused {
        // EmergencyUnpause is always allowed
        if !matches!(msg, ExecuteMsg::EmergencyUnpause {}) {
            return Err(ContractError::ContractPaused {});
        }
    }

    match msg {
        ExecuteMsg::EmergencyUnpause {} => execute_emergency_unpause(deps, info),
        ExecuteMsg::EmergencyPause {} => execute_emergency_pause(deps, info),
        ExecuteMsg::ExecuteFacility {} => execute_facility(deps, info),
        ExecuteMsg::ExpireFacility {} => execute_expire_facility(deps),
        ExecuteMsg::MarkSettled {} => execute_mark_settled(deps, info),
        ExecuteMsg::CloseFacility {} => execute_close_facility(deps, info),
        ExecuteMsg::SetCreditManager { credit_manager } => execute_set_credit_manager(deps, info, credit_manager),
        ExecuteMsg::DisburseToPsp { amount, recipient } => execute_disburse_to_psp(deps, env, info, amount, recipient),
        ExecuteMsg::SyncLpHolder { address } => execute_sync_lp_holder(deps, env, info, address),
        ExecuteMsg::Deposit {} => execute_deposit(deps, env, info),
        ExecuteMsg::RequestWithdraw {} => execute_request_withdraw(deps, env, info),
        ExecuteMsg::ClaimWithdraw { withdrawal_id } => execute_claim_withdraw(deps, env, info, withdrawal_id),
        ExecuteMsg::UpdateConfig {
            stablecoin_denom,
            admin,
        } => execute_update_config(deps, info, stablecoin_denom, admin),
        ExecuteMsg::AdminWithdraw { amount } => execute_admin_withdraw(deps, env, info, amount),
        ExecuteMsg::AdminDepositYield {
            principal_amount,
            yield_amount,
        } => execute_admin_deposit_yield(deps, env, info, principal_amount, yield_amount),
    }
}

fn execute_facility(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.admin {
        return Err(ContractError::Unauthorized {
            caller: info.sender.to_string(),
            expected: config.admin.to_string(),
        });
    }
        if info.sender != config.admin {
            return Err(ContractError::Unauthorized {
                caller: info.sender.to_string(),
                expected: config.admin.to_string(),
            });
        }

    let current_state = POOL_STATE.load(deps.storage)?;
    if !matches!(current_state, PoolState::Fundraising) {
        return Err(ContractError::InvalidState {
            expected: "Fundraising".to_string(),
            got: format!("{:?}", current_state),
        });
    }

    POOL_STATE.save(deps.storage, &PoolState::Active)?;

    Ok(Response::new()
        .add_attribute("method", "execute_facility")
        .add_attribute("admin", info.sender)
        .add_attribute("new_state", "Active"))
}

fn execute_expire_facility(deps: DepsMut) -> Result<Response, ContractError> {
    let current_state = POOL_STATE.load(deps.storage)?;
    if !matches!(current_state, PoolState::Active) {
        return Err(ContractError::InvalidState {
            expected: "Active".to_string(),
            got: format!("{:?}", current_state),
        });
    }

    POOL_STATE.save(deps.storage, &PoolState::WindingDown)?;

    Ok(Response::new()
        .add_attribute("method", "expire_facility")
        .add_attribute("new_state", "WindingDown"))
}

fn execute_mark_settled(
    deps: DepsMut,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.admin {
        return Err(ContractError::Unauthorized {
            caller: info.sender.to_string(),
            expected: config.admin.to_string(),
        });
    }
        if info.sender != config.admin {
            return Err(ContractError::Unauthorized {
                caller: info.sender.to_string(),
                expected: config.admin.to_string(),
            });
        }

    let current_state = POOL_STATE.load(deps.storage)?;
    if !matches!(current_state, PoolState::WindingDown) {
        return Err(ContractError::InvalidState {
            expected: "WindingDown".to_string(),
            got: format!("{:?}", current_state),
        });
    }

    let vault_state = VAULT_STATE.load(deps.storage)?;
    if !vault_state.total_pending_withdrawals.is_zero() {
        return Err(ContractError::InvalidState {
            expected: "WindingDown with zero pending withdrawals".to_string(),
            got: "WindingDown with pending withdrawals".to_string(),
        });
    }

    POOL_STATE.save(deps.storage, &PoolState::Settled)?;

    Ok(Response::new()
        .add_attribute("method", "mark_settled")
        .add_attribute("admin", info.sender)
        .add_attribute("new_state", "Settled"))
}

fn execute_close_facility(
    deps: DepsMut,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.admin {
        return Err(ContractError::Unauthorized {
            caller: info.sender.to_string(),
            expected: config.admin.to_string(),
        });
    }
        if info.sender != config.admin {
            return Err(ContractError::Unauthorized {
                caller: info.sender.to_string(),
                expected: config.admin.to_string(),
            });
        }

    let current_state = POOL_STATE.load(deps.storage)?;
    if !matches!(current_state, PoolState::Settled) {
        return Err(ContractError::InvalidState {
            expected: "Settled".to_string(),
            got: format!("{:?}", current_state),
        });
    }

    POOL_STATE.save(deps.storage, &PoolState::Closed)?;

    Ok(Response::new()
        .add_attribute("method", "close_facility")
        .add_attribute("admin", info.sender)
        .add_attribute("new_state", "Closed"))
}

fn execute_set_credit_manager(
    deps: DepsMut,
    info: MessageInfo,
    credit_manager: String,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender != config.admin {
        return Err(ContractError::Unauthorized {
            caller: info.sender.to_string(),
            expected: config.admin.to_string(),
        });
    }
        if info.sender != config.admin {
            return Err(ContractError::Unauthorized {
                caller: info.sender.to_string(),
                expected: config.admin.to_string(),
            });
        }

    let validated = deps.api.addr_validate(&credit_manager)?;
    config.credit_manager = Some(validated.clone());
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("method", "set_credit_manager")
        .add_attribute("admin", info.sender)
        .add_attribute("credit_manager", validated))
}

fn execute_disburse_to_psp(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
    recipient: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let credit_manager = config
        .credit_manager
        .clone()
        .ok_or(ContractError::CreditManagerNotConfigured {})?;
    if info.sender != credit_manager {
        return Err(ContractError::CreditManagerOnly {});
    }

    if !info.funds.is_empty() {
        return Err(ContractError::UnexpectedFunds {});
    }

    if amount.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let current_state = POOL_STATE.load(deps.storage)?;
    if !matches!(current_state, PoolState::Active) {
        return Err(ContractError::InvalidState {
            expected: "Active".to_string(),
            got: format!("{:?}", current_state),
        });
    }

    let recipient_addr = deps.api.addr_validate(&recipient)?;
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

    let transfer_msg = BankMsg::Send {
        to_address: recipient_addr.to_string(),
        amount: vec![Coin {
            denom: config.stablecoin_denom,
            amount,
        }],
    };

    Ok(Response::new()
        .add_message(transfer_msg)
        .add_attribute("method", "disburse_to_psp")
        .add_attribute("credit_manager", info.sender)
        .add_attribute("recipient", recipient_addr)
        .add_attribute("amount", amount))
}

fn execute_sync_lp_holder(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    address: String,
) -> Result<Response, ContractError> {
    if !info.funds.is_empty() {
        return Err(ContractError::UnexpectedFunds {});
    }

    let config = CONFIG.load(deps.storage)?;
    let holder = deps.api.addr_validate(&address)?;
    sync_lp_holder_index(deps, &env.contract.address, &config.lp_full_denom, &holder)?;

    Ok(Response::new()
        .add_attribute("method", "sync_lp_holder")
        .add_attribute("address", holder))
}

fn execute_emergency_pause(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.admin {
        return Err(ContractError::Unauthorized {
            caller: info.sender.to_string(),
            expected: config.admin.to_string(),
        });
    }

    PAUSED.save(deps.storage, &true)?;

    Ok(Response::new()
        .add_attribute("method", "emergency_pause")
        .add_attribute("admin", info.sender)
        .add_attribute("paused", "true"))
}

fn execute_emergency_unpause(
    deps: DepsMut,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.admin {
        return Err(ContractError::Unauthorized {
            caller: info.sender.to_string(),
            expected: config.admin.to_string(),
        });
    }
        if info.sender != config.admin {
            return Err(ContractError::Unauthorized {
                caller: info.sender.to_string(),
                expected: config.admin.to_string(),
            });
        }

    PAUSED.save(deps.storage, &false)?;

    Ok(Response::new()
        .add_attribute("method", "emergency_unpause")
        .add_attribute("admin", info.sender)
        .add_attribute("paused", "false"))
}

fn execute_deposit(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {

    // State guard: Only allow deposit if Fundraising, or (Active and backfill rules apply)
    let pool_state = POOL_STATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;
    let vault_state = VAULT_STATE.load(deps.storage)?;

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

    match pool_state {
        PoolState::Fundraising => {},
        PoolState::Active => {
            // Enforce backfill ceiling: execution_amount - total_deposited
            let remaining_capacity = config.execution_amount.checked_sub(vault_state.total_deposited)
                .map_err(|_| ContractError::OverflowError { operation: "backfill remaining_capacity".to_string() })?;
            if deposit_amount > remaining_capacity {
                return Err(ContractError::BackfillCapExceeded {
                    cap: remaining_capacity,
                    requested: deposit_amount,
                });
            }
        },
        _ => {
            return Err(ContractError::InvalidState {
                expected: "Fundraising or Active (backfill)".to_string(),
                got: format!("{:?}", pool_state),
            });
        }
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
    LP_HOLDERS.save(deps.storage, &info.sender, &true)?;

    // Update USER_INFOS: set deposit_timestamp if first deposit, update lp_balance
    use crate::state::USER_INFOS;
    let mut user_info = USER_INFOS.may_load(deps.storage, &info.sender)?.unwrap_or(crate::state::UserInfo {
        deposit_timestamp: env.block.time.seconds(),
        lp_balance: Uint128::zero(),
    });
    // If this is the first deposit, deposit_timestamp is set above
    user_info.lp_balance = user_info.lp_balance.checked_add(lp_to_mint).map_err(|_| ContractError::OverflowError { operation: "user lp_balance add".to_string() })?;
    USER_INFOS.save(deps.storage, &info.sender, &user_info)?;

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

    // State guard: Only allow withdrawal request if Active or WindingDown
    let pool_state = POOL_STATE.load(deps.storage)?;
    match pool_state {
        PoolState::Active | PoolState::WindingDown => {},
        _ => {
            return Err(ContractError::InvalidState {
                expected: "Active or WindingDown".to_string(),
                got: format!("{:?}", pool_state),
            });
        }
    }

    let current_day = day_of_week(env.block.time.seconds());
    if current_day != MONDAY_WITHDRAWAL_WINDOW_DAY {
        return Err(ContractError::WithdrawalWindowClosed { current_day });
    }

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

    let queue_id = WITHDRAWAL_QUEUE_NEXT.load(deps.storage)?;
    let next_queue_id = queue_id
        .checked_add(1)
        .ok_or(ContractError::OverflowError {
            operation: "withdrawal queue id".to_string(),
        })?;
    WITHDRAWAL_QUEUE_NEXT.save(deps.storage, &next_queue_id)?;

    // Calculate release time using immutable config.withdrawal_delay
    let release_time = env.block.time.plus_seconds(config.withdrawal_delay);

    // Create pending withdrawal
    let pending_withdrawal = PendingWithdrawal {
        queue_id,
        amount: stablecoin_amount,
        requested_at: env.block.time,
        release_time,
    };

    PENDING_WITHDRAWALS.save(deps.storage, (&user_addr, withdrawal_id), &pending_withdrawal)?;
    USER_WITHDRAWAL_QUEUE_ID.save(deps.storage, (&user_addr, withdrawal_id), &queue_id)?;
    WITHDRAWAL_QUEUE.save(
        deps.storage,
        queue_id,
        &QueueEntry {
            user: user_addr.clone(),
            withdrawal_id,
            requested_at: env.block.time,
        },
    )?;

    sync_lp_holder_index(
        deps,
        &env.contract.address,
        &config.lp_full_denom,
        &user_addr,
    )?;

    // Burn LP tokens via TokenFactory
    let burn_msg = burn_tokens_msg(
        env.contract.address.to_string(),
        config.lp_full_denom.clone(),
        lp_amount.to_string(),
    )?;

    // Calculate forfeited yield: principal × investor_apy_bps × 7 / (360 × 10_000)
    let forfeited_yield = lp_amount
        .u128()
        .saturating_mul(config.investor_apy_bps as u128)
        .saturating_mul(7)
        / (360 * 10_000);
    let forfeited_yield = Uint128::from(forfeited_yield);

    let mut response = Response::new()
        .add_message(burn_msg)
        .add_attribute("method", "request_withdraw")
        .add_attribute("user", user_addr.clone())
        .add_attribute("withdrawal_id", withdrawal_id.to_string())
        .add_attribute("queue_id", queue_id.to_string())
        .add_attribute("lp_burned", lp_amount)
        .add_attribute("stablecoin_amount_with_yield", stablecoin_amount)
        .add_attribute("release_time", release_time.to_string())
        .add_attribute("vault_total_deposited", vault_state.total_deposited);

    if !forfeited_yield.is_zero() {
        use cosmwasm_std::{WasmMsg, to_json_binary};
        let reserve_msg = WasmMsg::Execute {
            contract_addr: config.yield_reserve.to_string(),
            msg: to_json_binary(&yield_reserve_msg::ExecuteMsg::ReceiveFrom { amount: forfeited_yield })?,
            funds: vec![Coin {
                denom: config.stablecoin_denom.clone(),
                amount: forfeited_yield,
            }],
        };
        response = response.add_message(reserve_msg);
        response = response.add_attribute("forfeited_yield", forfeited_yield);
    }

    Ok(response)
}

fn execute_claim_withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    withdrawal_id: u64,
) -> Result<Response, ContractError> {

    // State guard: Only allow claim if Active, WindingDown, or Settled
    let pool_state = POOL_STATE.load(deps.storage)?;
    match pool_state {
        PoolState::Active | PoolState::WindingDown | PoolState::Settled => {},
        _ => {
            return Err(ContractError::InvalidState {
                expected: "Active, WindingDown, or Settled".to_string(),
                got: format!("{:?}", pool_state),
            });
        }
    }

    // Security: No funds should be sent with claim
    if !info.funds.is_empty() {
        return Err(ContractError::Std(cosmwasm_std::StdError::generic_err(
            "Do not send funds when claiming withdrawal",
        )));
    }

    let config = CONFIG.load(deps.storage)?;
    let user_addr = info.sender.clone();

    let queue_id = USER_WITHDRAWAL_QUEUE_ID
        .may_load(deps.storage, (&user_addr, withdrawal_id))?
        .ok_or(ContractError::WithdrawalNotFound {})?;
    let queue_head = WITHDRAWAL_QUEUE_HEAD.load(deps.storage)?;
    if queue_id != queue_head {
        return Err(ContractError::QueueOrderViolation {
            head_queue_id: queue_head,
            requested_queue_id: queue_id,
        });
    }

    WITHDRAWAL_QUEUE
        .may_load(deps.storage, queue_id)?
        .ok_or(ContractError::QueueEntryNotFound { queue_id })?;

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
    USER_WITHDRAWAL_QUEUE_ID.remove(deps.storage, (&user_addr, withdrawal_id));
    WITHDRAWAL_QUEUE.remove(deps.storage, queue_id);

    let new_head = queue_head
        .checked_add(1)
        .ok_or(ContractError::OverflowError {
            operation: "queue head advance".to_string(),
        })?;
    WITHDRAWAL_QUEUE_HEAD.save(deps.storage, &new_head)?;

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
        .add_attribute("queue_id", queue_id.to_string())
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
        return Err(ContractError::Unauthorized {
            caller: info.sender.to_string(),
            expected: config.admin.to_string(),
        });
    }
        if info.sender != config.admin {
            return Err(ContractError::Unauthorized {
                caller: info.sender.to_string(),
                expected: config.admin.to_string(),
            });
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
        return Err(ContractError::Unauthorized {
            caller: info.sender.to_string(),
            expected: config.admin.to_string(),
        });
    }
        if info.sender != config.admin {
            return Err(ContractError::Unauthorized {
                caller: info.sender.to_string(),
                expected: config.admin.to_string(),
            });
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
        return Err(ContractError::Unauthorized {
            caller: info.sender.to_string(),
            expected: config.admin.to_string(),
        });
    }
        if info.sender != config.admin {
            return Err(ContractError::Unauthorized {
                caller: info.sender.to_string(),
                expected: config.admin.to_string(),
            });
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
        QueryMsg::PoolState {} => to_json_binary(&query_pool_state(deps)?),
        QueryMsg::OldestQueueWaitDays {} => to_json_binary(&query_oldest_queue_wait_days(deps, env)?),
        QueryMsg::LpHolders { start_after, limit } => {
            to_json_binary(&query_lp_holders(deps, start_after, limit)?)
        }
    }
}

fn query_pool_state(deps: Deps) -> StdResult<PoolStateResponse> {
    let state = POOL_STATE.load(deps.storage)?;
    let paused = PAUSED.may_load(deps.storage)?.unwrap_or(false);
    
    Ok(PoolStateResponse {
        state: format!("{:?}", state),
        paused,
    })
}

fn query_lp_holders(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<LpHoldersResponse> {
    let limit = limit.unwrap_or(50);
    if limit == 0 {
        return Err(cosmwasm_std::StdError::generic_err(
            ContractError::InvalidPaginationLimit {}.to_string(),
        ));
    }
    let limit = limit.min(200) as usize;

    let start_after = match start_after {
        Some(raw) => Some(deps.api.addr_validate(&raw)?.to_string()),
        None => None,
    };

    let holders = LP_HOLDERS
        .range(deps.storage, None, None, Order::Ascending)
        .filter_map(|item| match item {
            Ok((addr, true)) => Some(Ok(addr.to_string())),
            Ok((_addr, false)) => None,
            Err(err) => Some(Err(err)),
        })
        .filter_map(|item| match item {
            Ok(addr) => {
                if let Some(start_after) = &start_after {
                    if addr <= *start_after {
                        return None;
                    }
                }
                Some(Ok(addr))
            }
            Err(err) => Some(Err(err)),
        })
        .take(limit)
        .collect::<StdResult<Vec<String>>>()?;

    Ok(LpHoldersResponse { holders })
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        stablecoin_denom: config.stablecoin_denom,
        lp_full_denom: config.lp_full_denom,
        admin: config.admin,
        withdrawal_delay: config.withdrawal_delay,
        credit_manager: config.credit_manager.map(|addr| addr.to_string()),
    })
}

fn query_oldest_queue_wait_days(deps: Deps, env: Env) -> StdResult<OldestQueueWaitDaysResponse> {
    let queue_head = WITHDRAWAL_QUEUE_HEAD.load(deps.storage)?;
    let queue_next = WITHDRAWAL_QUEUE_NEXT.load(deps.storage)?;
    if queue_head >= queue_next {
        return Ok(OldestQueueWaitDaysResponse { days: None });
    }

    let maybe_entry = WITHDRAWAL_QUEUE.may_load(deps.storage, queue_head)?;
    let days = match maybe_entry {
        Some(entry) => {
            let now_secs = env.block.time.seconds();
            let requested_secs = entry.requested_at.seconds();
            let elapsed_secs = now_secs.checked_sub(requested_secs).ok_or_else(|| {
                cosmwasm_std::StdError::generic_err(format!(
                    "block time ({}) is before withdrawal request time ({})",
                    now_secs, requested_secs
                ))
            })?;
            Some(elapsed_secs / SECONDS_PER_DAY)
        }
        None => None,
    };

    Ok(OldestQueueWaitDaysResponse { days })
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

fn sync_lp_holder_index(
    deps: DepsMut,
    contract_addr: &Addr,
    lp_full_denom: &str,
    holder: &Addr,
) -> Result<(), ContractError> {
    let balance = query_bank_balance(&deps.querier, contract_addr, lp_full_denom)?;
    if balance.is_zero() {
        LP_HOLDERS.remove(deps.storage, holder);
    } else {
        LP_HOLDERS.save(deps.storage, holder, &true)?;
    }

    Ok(())
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

    use crate::state::USER_INFOS;
    let deposit_timestamp = USER_INFOS.may_load(deps.storage, &user_addr)?.map(|u| u.deposit_timestamp).unwrap_or(0);
    Ok(UserInfoResponse {
        address: user_addr,
        lp_balance,
        stablecoin_value,
        deposit_timestamp,
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
                queue_id: withdrawal.queue_id,
                amount: withdrawal.amount,
                requested_at: withdrawal.requested_at,
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
            queue_id: withdrawal.queue_id,
            amount: withdrawal.amount,
            requested_at: withdrawal.requested_at,
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
