use cosmwasm_std::{
    entry_point, to_json_binary, Addr, BankMsg, Binary, Coin, Deps, DepsMut, Env, MessageInfo,
    Order, QuerierWrapper, Response, StdResult, Uint128,
};
use cw2::set_contract_version;

use crate::custom::{burn_tokens_msg, create_denom_msg, mint_and_send_tokens_msg};
use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, PendingWithdrawalsResponse,
    QueryMsg, UserInfoResponse, VaultInfoResponse, WithdrawalInfo, WithdrawalResponse,
};
use crate::state::{
    Config, PendingWithdrawal, VaultState, WithdrawalCounter, CONFIG, CONTRACT_NAME,
    CONTRACT_VERSION, MAX_WITHDRAWAL_DELAY, MIN_WITHDRAWAL_DELAY,
    PENDING_WITHDRAWALS, VAULT_STATE, WITHDRAWAL_COUNTERS,
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
    if withdrawal_delay < MIN_WITHDRAWAL_DELAY || withdrawal_delay > MAX_WITHDRAWAL_DELAY {
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
    let config = Config {
        stablecoin_denom: msg.stablecoin_denom.clone(),
        lp_full_denom: lp_full_denom.clone(),
        admin: admin_addr.clone(),
        withdrawal_delay,
    };
    CONFIG.save(deps.storage, &config)?;

    // Initialize vault state
    let vault_state = VaultState {
        total_stablecoin_deposited: Uint128::zero(),
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

    let amount = stablecoin_sent.amount;
    if amount.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    // For 1:1 exchange, LP amount equals deposit amount
    let lp_amount = amount;

    // Load vault state
    let mut vault_state = VAULT_STATE.load(deps.storage)?;
    
    // Update vault state with checked arithmetic
    vault_state.total_stablecoin_deposited = vault_state
        .total_stablecoin_deposited
        .checked_add(amount)
        .map_err(|_| ContractError::OverflowError {
            operation: "deposit".to_string(),
        })?;

    vault_state.total_lp_minted = vault_state
        .total_lp_minted
        .checked_add(lp_amount)
        .map_err(|_| ContractError::OverflowError {
            operation: "LP mint".to_string(),
        })?;

    // Security check: Ensure accounting invariant holds
    // total_deposited >= total_lp_minted (should be equal for 1:1)
    // We allow >= to handle edge case where there might be pending withdrawals
    if vault_state.total_stablecoin_deposited
        < vault_state.total_lp_minted.checked_add(vault_state.total_pending_withdrawals).unwrap_or(Uint128::MAX)
    {
        return Err(ContractError::InconsistentVaultState {});
    }

    // Save state before external call
    VAULT_STATE.save(deps.storage, &vault_state)?;

    // Mint LP tokens to the user via TokenFactory
    let mint_msg = mint_and_send_tokens_msg(
        env.contract.address.to_string(),
        config.lp_full_denom.clone(),
        lp_amount.to_string(),
        info.sender.to_string(),
    )?;

    Ok(Response::new()
        .add_message(mint_msg)
        .add_attribute("method", "deposit")
        .add_attribute("user", info.sender)
        .add_attribute("stablecoin_amount", amount)
        .add_attribute("lp_amount", lp_amount))
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

    // For 1:1 exchange, stablecoin amount equals LP amount
    let stablecoin_amount = lp_amount;

    // Load vault state for validation
    let mut vault_state = VAULT_STATE.load(deps.storage)?;

    // Security check: Ensure LP tokens being burned don't exceed total minted
    if vault_state.total_lp_minted < lp_amount {
        return Err(ContractError::Std(cosmwasm_std::StdError::generic_err(
            "LP amount exceeds total minted supply",
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

    // Security check: Ensure accounting invariant
    // total_deposited >= total_pending (we keep funds for pending withdrawals)
    if vault_state.total_stablecoin_deposited < vault_state.total_pending_withdrawals {
        return Err(ContractError::InconsistentVaultState {});
    }

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
    let release_time = env
        .block
        .time
        .plus_seconds(config.withdrawal_delay);

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
        .add_attribute("lp_amount", lp_amount)
        .add_attribute("stablecoin_amount", stablecoin_amount)
        .add_attribute("release_time", release_time.to_string()))
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

    // Security check: Ensure vault has sufficient funds
    if vault_state.total_stablecoin_deposited < stablecoin_amount {
        return Err(ContractError::InsufficientVaultBalance {});
    }

    // Security check: Ensure pending withdrawals accounting is correct
    if vault_state.total_pending_withdrawals < stablecoin_amount {
        return Err(ContractError::Std(cosmwasm_std::StdError::generic_err(
            "Pending withdrawals accounting mismatch",
        )));
    }

    // Update vault state BEFORE external call (reentrancy protection)
    vault_state.total_stablecoin_deposited = vault_state
        .total_stablecoin_deposited
        .checked_sub(stablecoin_amount)
        .map_err(|_| ContractError::OverflowError {
            operation: "withdrawal".to_string(),
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
        .add_attribute("stablecoin_amount", stablecoin_amount))
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

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&query_config(deps)?),
        QueryMsg::VaultInfo {} => to_json_binary(&query_vault_info(deps)?),
        QueryMsg::UserInfo { address } => to_json_binary(&query_user_info(deps, address)?),
        QueryMsg::PendingWithdrawals { address } => {
            to_json_binary(&query_pending_withdrawals(deps, env, address)?)
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

fn query_vault_info(deps: Deps) -> StdResult<VaultInfoResponse> {
    let vault_state = VAULT_STATE.load(deps.storage)?;

    Ok(VaultInfoResponse {
        total_stablecoin_deposited: vault_state.total_stablecoin_deposited,
        total_lp_supply: vault_state.total_lp_minted,
        total_pending_withdrawals: vault_state.total_pending_withdrawals,
    })
}

fn query_user_info(deps: Deps, address: String) -> StdResult<UserInfoResponse> {
    let config = CONFIG.load(deps.storage)?;
    let user_addr = deps.api.addr_validate(&address)?;

    // Query LP balance from bank module
    let lp_balance = query_bank_balance(&deps.querier, &user_addr, &config.lp_full_denom)?;

    // For 1:1 exchange, stablecoin value equals LP balance
    let stablecoin_value = lp_balance;

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

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, from_json};

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: Some(false),
            uri: None,
            uri_hash: None,
            description: Some("LP Token for Liquidity Pool".to_string()),
            admin: None,
            withdrawal_delay_seconds: 172_800, // 2 days
        };
        let info = mock_info("creator", &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(1, res.messages.len());

        let res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
        let config: ConfigResponse = from_json(&res).unwrap();
        assert_eq!("uzig", config.stablecoin_denom);
        assert!(config.lp_full_denom.contains("lplp"));
        assert_eq!(config.admin, Addr::unchecked("creator"));
        assert_eq!(172_800, config.withdrawal_delay);
    }

    #[test]
    fn test_invalid_subdenom() {
        let mut deps = mock_dependencies();

        // Too short
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "ab".to_string(),
            lp_minting_cap: Uint128::new(1_000_000),
            can_change_minting_cap: None,
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120, // 2 minutes (minimum)
        };
        let info = mock_info("creator", &[]);
        let err = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap_err();
        assert!(matches!(err, ContractError::InvalidSubdenom { .. }));

        // Doesn't start with lowercase
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "ABC".to_string(),
            lp_minting_cap: Uint128::new(1_000_000),
            can_change_minting_cap: None,
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120, // 2 minutes (minimum)
        };
        let err = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert!(matches!(err, ContractError::InvalidSubdenom { .. }));
    }

    #[test]
    fn test_zero_minting_cap() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::zero(),
            can_change_minting_cap: None,
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120, // 2 minutes (minimum)
        };
        let info = mock_info("creator", &[]);
        let err = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert!(matches!(err, ContractError::InvalidMintingCap {}));
    }

    #[test]
    fn test_withdrawal_delay_validation() {
        let mut deps = mock_dependencies();
        
        // Test too short delay (less than 2 minutes)
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000),
            can_change_minting_cap: None,
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 100, // Too short (< 120)
        };
        let info = mock_info("creator", &[]);
        let err = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap_err();
        assert!(matches!(err, ContractError::InvalidWithdrawalDelay { .. }));

        // Test too long delay
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000),
            can_change_minting_cap: None,
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 10_000_000, // Too long
        };
        let err = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap_err();
        assert!(matches!(err, ContractError::InvalidWithdrawalDelay { .. }));

        // Test valid minimum delay (2 minutes)
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000),
            can_change_minting_cap: None,
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120, // Exactly at minimum - valid
        };
        let res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        assert_eq!(1, res.messages.len());
        
        // Verify the delay was set
        let config_res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
        let config: ConfigResponse = from_json(&config_res).unwrap();
        assert_eq!(120, config.withdrawal_delay);

        // Test valid custom delay (1 day)
        let mut deps2 = mock_dependencies();
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000),
            can_change_minting_cap: None,
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 86400, // 1 day - valid
        };
        let res = instantiate(deps2.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(1, res.messages.len());
        
        // Verify the delay was set
        let config_res = query(deps2.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
        let config: ConfigResponse = from_json(&config_res).unwrap();
        assert_eq!(86400, config.withdrawal_delay);
    }

    #[test]
    fn test_time_locked_withdrawal_flow() {
        let mut deps = mock_dependencies();
        let mut env = mock_env();

        // Initialize contract with custom delay for faster testing
        let custom_delay = 7200; // 2 hours
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: Some(false),
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: custom_delay,
        };
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

        // Get the LP denom
        let config_res = query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap();
        let config: ConfigResponse = from_json(&config_res).unwrap();
        let lp_denom = config.lp_full_denom;
        assert_eq!(custom_delay, config.withdrawal_delay);

        // Test deposit (simulated - in real chain TokenFactory would mint)
        let info = mock_info("user", &coins(1000, "uzig"));
        let res = execute(deps.as_mut(), env.clone(), info, ExecuteMsg::Deposit {}).unwrap();
        assert_eq!(1, res.messages.len());

        // Test request withdrawal
        let info = mock_info("user", &coins(500, &lp_denom));
        let res = execute(
            deps.as_mut(),
            env.clone(),
            info,
            ExecuteMsg::RequestWithdraw {},
        )
        .unwrap();
        assert_eq!(1, res.messages.len());

        // Check vault state includes pending withdrawals
        let vault_info_res = query(deps.as_ref(), env.clone(), QueryMsg::VaultInfo {}).unwrap();
        let vault_info: VaultInfoResponse = from_json(&vault_info_res).unwrap();
        assert_eq!(Uint128::new(500), vault_info.total_pending_withdrawals);

        // Query pending withdrawals directly from storage
        let user_addr = Addr::unchecked("user");
        let pending_withdrawal = PENDING_WITHDRAWALS
            .load(deps.as_ref().storage, (&user_addr, 0))
            .unwrap();
        assert_eq!(Uint128::new(500), pending_withdrawal.amount);
        assert!(env.block.time < pending_withdrawal.release_time);

        // Try to claim too early
        let info = mock_info("user", &[]);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ClaimWithdraw { withdrawal_id: 0 },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::WithdrawalLocked { .. }));

        // Advance time by custom delay
        env.block.time = env.block.time.plus_seconds(custom_delay);

        // Now claim should succeed
        let res = execute(
            deps.as_mut(),
            env.clone(),
            info,
            ExecuteMsg::ClaimWithdraw { withdrawal_id: 0 },
        )
        .unwrap();
        assert_eq!(1, res.messages.len());

        // Verify withdrawal is removed
        let withdrawal_exists = PENDING_WITHDRAWALS
            .may_load(deps.as_ref().storage, (&user_addr, 0))
            .unwrap();
        assert!(withdrawal_exists.is_none());

        // Verify vault state updated
        let vault_info_res = query(deps.as_ref(), env.clone(), QueryMsg::VaultInfo {}).unwrap();
        let vault_info: VaultInfoResponse = from_json(&vault_info_res).unwrap();
        assert_eq!(Uint128::zero(), vault_info.total_pending_withdrawals);
        assert_eq!(Uint128::new(500), vault_info.total_stablecoin_deposited);
    }
}
