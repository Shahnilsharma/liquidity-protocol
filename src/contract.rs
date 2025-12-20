use cosmwasm_std::{
    entry_point, to_json_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo, Response,
    StdResult, Uint128, WasmMsg, CosmosMsg, StdError,
};
use cw2::set_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20QueryMsg, BalanceResponse};

use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, PoolInfoResponse, QueryMsg,
    UserInfoResponse,
};
use crate::state::{Config, PoolState, CONFIG, CONTRACT_NAME, CONTRACT_VERSION, POOL_STATE};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    // Validate addresses
    let stablecoin_addr = deps.api.addr_validate(&msg.stablecoin_address)?;
    let lp_token_addr = deps.api.addr_validate(&msg.lp_token_address)?;
    let admin_addr = match msg.admin {
        Some(addr) => deps.api.addr_validate(&addr)?,
        None => info.sender.clone(),
    };

    // Initialize configuration
    let config = Config {
        stablecoin_address: stablecoin_addr.clone(),
        lp_token_address: lp_token_addr.clone(),
        admin: admin_addr.clone(),
    };
    CONFIG.save(deps.storage, &config)?;

    // Initialize pool state
    let pool_state = PoolState {
        total_stablecoin_deposited: Uint128::zero(),
        total_lp_minted: Uint128::zero(),
    };
    POOL_STATE.save(deps.storage, &pool_state)?;

    // Set contract version for migration
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("stablecoin_address", stablecoin_addr)
        .add_attribute("lp_token_address", lp_token_addr)
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
        ExecuteMsg::Deposit { amount } => execute_deposit(deps, env, info, amount),
        ExecuteMsg::Withdraw { amount } => execute_withdraw(deps, env, info, amount),
        ExecuteMsg::UpdateConfig {
            stablecoin_address,
            lp_token_address,
            admin,
        } => execute_update_config(deps, info, stablecoin_address, lp_token_address, admin),
    }
}

fn execute_deposit(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    if amount.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let config = CONFIG.load(deps.storage)?;
    let mut pool_state = POOL_STATE.load(deps.storage)?;

    let transfer_from_msg = Cw20ExecuteMsg::TransferFrom {
        owner: info.sender.to_string(),
        recipient: env.contract.address.to_string(),
        amount,
    };

    let transfer_cosmos_msg: CosmosMsg = WasmMsg::Execute {
        contract_addr: config.stablecoin_address.to_string(),
        msg: to_json_binary(&transfer_from_msg)?,
        funds: vec![],
    }
    .into();

    let lp_amount = amount;

    let mint_msg = Cw20ExecuteMsg::Mint {
        recipient: info.sender.to_string(),
        amount: lp_amount,
    };

    let mint_cosmos_msg: CosmosMsg = WasmMsg::Execute {
        contract_addr: config.lp_token_address.to_string(),
        msg: to_json_binary(&mint_msg)?,
        funds: vec![],
    }
    .into();

    pool_state.total_stablecoin_deposited = pool_state
        .total_stablecoin_deposited
        .checked_add(amount)
        .map_err(|_| StdError::generic_err("Overflow in deposit"))?;
    
    pool_state.total_lp_minted = pool_state
        .total_lp_minted
        .checked_add(lp_amount)
        .map_err(|_| StdError::generic_err("Overflow in LP mint"))?;

    POOL_STATE.save(deps.storage, &pool_state)?;

    Ok(Response::new()
        .add_message(transfer_cosmos_msg)
        .add_message(mint_cosmos_msg)
        .add_attribute("method", "deposit")
        .add_attribute("user", info.sender)
        .add_attribute("stablecoin_amount", amount)
        .add_attribute("lp_amount", lp_amount))
}

fn execute_withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    lp_amount: Uint128,
) -> Result<Response, ContractError> {
    if lp_amount.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let config = CONFIG.load(deps.storage)?;
    let mut pool_state = POOL_STATE.load(deps.storage)?;

    let lp_balance = query_cw20_balance(
        deps.as_ref(),
        &config.lp_token_address,
        &info.sender,
    )?;

    if lp_balance < lp_amount {
        return Err(ContractError::InsufficientLpBalance {});
    }

    let stablecoin_amount = lp_amount;

    if pool_state.total_stablecoin_deposited < stablecoin_amount {
        return Err(ContractError::InsufficientPoolBalance {});
    }

    let transfer_lp_msg = Cw20ExecuteMsg::TransferFrom {
        owner: info.sender.to_string(),
        recipient: env.contract.address.to_string(),
        amount: lp_amount,
    };

    let transfer_lp_cosmos_msg: CosmosMsg = WasmMsg::Execute {
        contract_addr: config.lp_token_address.to_string(),
        msg: to_json_binary(&transfer_lp_msg)?,
        funds: vec![],
    }
    .into();

    let burn_msg = Cw20ExecuteMsg::Burn {
        amount: lp_amount,
    };

    let burn_cosmos_msg: CosmosMsg = WasmMsg::Execute {
        contract_addr: config.lp_token_address.to_string(),
        msg: to_json_binary(&burn_msg)?,
        funds: vec![],
    }
    .into();

    let transfer_msg = Cw20ExecuteMsg::Transfer {
        recipient: info.sender.to_string(),
        amount: stablecoin_amount,
    };

    let transfer_cosmos_msg: CosmosMsg = WasmMsg::Execute {
        contract_addr: config.stablecoin_address.to_string(),
        msg: to_json_binary(&transfer_msg)?,
        funds: vec![],
    }
    .into();

    pool_state.total_stablecoin_deposited = pool_state
        .total_stablecoin_deposited
        .checked_sub(stablecoin_amount)
        .map_err(|_| StdError::generic_err("Underflow in withdrawal"))?;
    
    pool_state.total_lp_minted = pool_state
        .total_lp_minted
        .checked_sub(lp_amount)
        .map_err(|_| StdError::generic_err("Underflow in LP burn"))?;

    POOL_STATE.save(deps.storage, &pool_state)?;

    Ok(Response::new()
        .add_message(transfer_lp_cosmos_msg)
        .add_message(burn_cosmos_msg)
        .add_message(transfer_cosmos_msg)
        .add_attribute("method", "withdraw")
        .add_attribute("user", info.sender)
        .add_attribute("lp_amount", lp_amount)
        .add_attribute("stablecoin_amount", stablecoin_amount))
}

fn execute_update_config(
    deps: DepsMut,
    info: MessageInfo,
    stablecoin_address: Option<String>,
    lp_token_address: Option<String>,
    admin: Option<String>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    if info.sender != config.admin {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(addr) = stablecoin_address {
        config.stablecoin_address = deps.api.addr_validate(&addr)?;
    }

    if let Some(addr) = lp_token_address {
        config.lp_token_address = deps.api.addr_validate(&addr)?;
    }

    if let Some(addr) = admin {
        config.admin = deps.api.addr_validate(&addr)?;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("method", "update_config")
        .add_attribute("admin", info.sender))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&query_config(deps)?),
        QueryMsg::PoolInfo {} => to_json_binary(&query_pool_info(deps)?),
        QueryMsg::UserInfo { address } => to_json_binary(&query_user_info(deps, address)?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        stablecoin_address: config.stablecoin_address,
        lp_token_address: config.lp_token_address,
        admin: config.admin,
    })
}

fn query_pool_info(deps: Deps) -> StdResult<PoolInfoResponse> {
    let pool_state = POOL_STATE.load(deps.storage)?;
    
    let exchange_rate = if pool_state.total_lp_minted.is_zero() {
        "1.0".to_string()
    } else {
        let rate = pool_state.total_stablecoin_deposited.u128() as f64
            / pool_state.total_lp_minted.u128() as f64;
        format!("{:.6}", rate)
    };

    Ok(PoolInfoResponse {
        total_stablecoin_deposited: pool_state.total_stablecoin_deposited,
        total_lp_supply: pool_state.total_lp_minted,
        exchange_rate,
    })
}

fn query_user_info(deps: Deps, address: String) -> StdResult<UserInfoResponse> {
    let config = CONFIG.load(deps.storage)?;
    let user_addr = deps.api.addr_validate(&address)?;

    let lp_balance = query_cw20_balance(deps, &config.lp_token_address, &user_addr)?;
    let stablecoin_value = lp_balance;

    Ok(UserInfoResponse {
        address: user_addr,
        lp_balance,
        stablecoin_value,
    })
}

fn query_cw20_balance(deps: Deps, token_addr: &Addr, user_addr: &Addr) -> StdResult<Uint128> {
    let balance_query = Cw20QueryMsg::Balance {
        address: user_addr.to_string(),
    };

    let balance_response: BalanceResponse = deps.querier.query_wasm_smart(
        token_addr.to_string(),
        &balance_query,
    )?;

    Ok(balance_response.balance)
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
            stablecoin_address: "stable_token".to_string(),
            lp_token_address: "lp_token".to_string(),
            admin: None,
        };
        let info = mock_info("creator", &coins(1000, "uzig"));
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());

        // Query config
        let res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
        let config: ConfigResponse = from_json(&res).unwrap();
        assert_eq!("stable_token", config.stablecoin_address);
        assert_eq!("lp_token", config.lp_token_address);
        assert_eq!("creator", config.admin);
    }

    #[test]
    fn test_zero_amount_deposit() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            stablecoin_address: "stable_token".to_string(),
            lp_token_address: "lp_token".to_string(),
            admin: None,
        };
        let info = mock_info("creator", &[]);
        instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        // Try to deposit zero amount
        let deposit_msg = ExecuteMsg::Deposit {
            amount: Uint128::zero(),
        };
        let err = execute(deps.as_mut(), mock_env(), info, deposit_msg).unwrap_err();
        match err {
            ContractError::InvalidZeroAmount {} => {}
            _ => panic!("Expected InvalidZeroAmount error"),
        }
    }
}
