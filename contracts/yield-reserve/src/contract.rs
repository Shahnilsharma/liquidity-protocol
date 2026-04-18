use serde::Deserialize;
// Minimal local QueryMsg for PSPPool PoolState query
#[derive(serde::Serialize)]
enum PSPPoolQueryMsg {
    PoolState {},
}

// Lightweight struct for PSPPool state query response
#[derive(serde::Serialize, serde::Deserialize)]
struct PoolStateResponse {
    state: String,
    paused: bool,
}
use defa_types::addr_validate_bypass::addr_validate_bypass;
use cosmwasm_std::{
    entry_point, to_json_binary, BankMsg, Binary, Coin, Deps, DepsMut, Env,
    MessageInfo, QuerierWrapper, Response, StdResult, Uint128,
};
use cw2::set_contract_version;

use crate::error::ContractError;
use crate::msg::{
    AccountingResponse, BalanceResponse, ConfigResponse, ExecuteMsg, InstantiateMsg,
    MigrateMsg, QueryMsg,
};
use crate::state::{
    Config, ADMIN_TOPUPS, CONFIG, CONTRACT_NAME, CONTRACT_VERSION,
    FEES_COLLECTED, LP_YIELD_DISBURSED, PROTOCOL_REVENUE_CLAIMED,
};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    #[cfg(feature = "test-addr-bypass")]
    let admin = cosmwasm_std::Addr::unchecked(addr_validate_bypass(&msg.admin).map_err(|e| cosmwasm_std::StdError::generic_err(e))?);
    #[cfg(not(feature = "test-addr-bypass"))]
    let admin = deps.api.addr_validate(&msg.admin)?;

    #[cfg(feature = "test-addr-bypass")]
    let credit_manager = cosmwasm_std::Addr::unchecked(addr_validate_bypass(&msg.credit_manager).map_err(|e| cosmwasm_std::StdError::generic_err(e))?);
    #[cfg(not(feature = "test-addr-bypass"))]
    let credit_manager = deps.api.addr_validate(&msg.credit_manager)?;

    #[cfg(feature = "test-addr-bypass")]
    let yield_distributor = cosmwasm_std::Addr::unchecked(addr_validate_bypass(&msg.yield_distributor).map_err(|e| cosmwasm_std::StdError::generic_err(e))?);
    #[cfg(not(feature = "test-addr-bypass"))]
    let yield_distributor = deps.api.addr_validate(&msg.yield_distributor)?;

    #[cfg(feature = "test-addr-bypass")]
    let psp_pool = cosmwasm_std::Addr::unchecked(addr_validate_bypass(&msg.psp_pool).map_err(|e| cosmwasm_std::StdError::generic_err(e))?);
    #[cfg(not(feature = "test-addr-bypass"))]
    let psp_pool = deps.api.addr_validate(&msg.psp_pool)?;

    CONFIG.save(
        deps.storage,
        &Config {
            admin: admin.clone(),
            credit_manager: credit_manager.clone(),
            yield_distributor: yield_distributor.clone(),
            stablecoin_denom: msg.stablecoin_denom.clone(),
            psp_pool: psp_pool.clone(),
        },
    )?;
    FEES_COLLECTED.save(deps.storage, &Uint128::zero())?;
    ADMIN_TOPUPS.save(deps.storage, &Uint128::zero())?;
    LP_YIELD_DISBURSED.save(deps.storage, &Uint128::zero())?;
    PROTOCOL_REVENUE_CLAIMED.save(deps.storage, &Uint128::zero())?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("admin", admin)
        .add_attribute("credit_manager", credit_manager)
        .add_attribute("yield_distributor", yield_distributor)
        .add_attribute("stablecoin_denom", msg.stablecoin_denom)
        .add_attribute("psp_pool", psp_pool))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::ReceiveFrom { amount } => execute_receive_from(deps, info, amount),
        ExecuteMsg::Disburse { recipient, amount } => {
            execute_disburse(deps, env, info, recipient, amount)
        }
        ExecuteMsg::UpdateWiring {
            credit_manager,
            yield_distributor,
        } => execute_update_wiring(deps, info, credit_manager, yield_distributor),
        ExecuteMsg::AdminTopUp { amount } => execute_admin_top_up(deps, info, amount),
        ExecuteMsg::ClaimProtocolRevenue { amount } => {
            execute_claim_protocol_revenue(deps, env, info, amount)
        }
    }
}

fn execute_update_wiring(
    deps: DepsMut,
    info: MessageInfo,
    credit_manager: Option<String>,
    yield_distributor: Option<String>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    require_admin(&info, &config)?;
    require_no_funds(&info)?;

    let mut response = Response::new()
        .add_attribute("action", "update_wiring")
        .add_attribute("admin", info.sender.to_string());

    if let Some(credit_manager_addr) = credit_manager {
        let validated = deps.api.addr_validate(&credit_manager_addr)?;
        config.credit_manager = validated.clone();
        response = response.add_attribute("credit_manager", validated);
    }

    if let Some(yield_distributor_addr) = yield_distributor {
        let validated = deps.api.addr_validate(&yield_distributor_addr)?;
        config.yield_distributor = validated.clone();
        response = response.add_attribute("yield_distributor", validated);
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(response)
}

fn execute_receive_from(
    deps: DepsMut,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    require_credit_manager(&info, &config)?;

    if amount.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    require_exact_single_fund(&info, &config.stablecoin_denom, amount)?;

    let fees_collected = FEES_COLLECTED.load(deps.storage)?;
    let updated_fees_collected = checked_add(
        fees_collected,
        amount,
        "fees_collected + receive_from amount",
    )?;
    FEES_COLLECTED.save(deps.storage, &updated_fees_collected)?;

    Ok(Response::new()
        .add_attribute("action", "receive_from")
        .add_attribute("credit_manager", info.sender)
        .add_attribute("amount", amount)
        .add_attribute("fees_collected", updated_fees_collected))
}

fn execute_disburse(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    require_yield_distributor(&info, &config)?;

    if amount.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }
    require_no_funds(&info)?;

    let recipient_addr = deps.api.addr_validate(&recipient)?;

    let accounted_available = query_accounted_available(deps.as_ref())?;
    if accounted_available < amount {
        return Err(ContractError::InsufficientAccountedBalance {
            available: accounted_available,
            required: amount,
        });
    }

    let contract_balance = query_contract_balance(
        &deps.querier,
        env.contract.address.as_ref(),
        &config.stablecoin_denom,
    )?;
    if contract_balance < amount {
        return Err(ContractError::InsufficientContractBalance {
            available: contract_balance,
            required: amount,
        });
    }

    let disbursed = LP_YIELD_DISBURSED.load(deps.storage)?;
    let updated_disbursed = checked_add(disbursed, amount, "lp_yield_disbursed + amount")?;
    LP_YIELD_DISBURSED.save(deps.storage, &updated_disbursed)?;

    let send_msg = BankMsg::Send {
        to_address: recipient_addr.to_string(),
        amount: vec![Coin {
            denom: config.stablecoin_denom,
            amount,
        }],
    };

    Ok(Response::new()
        .add_message(send_msg)
        .add_attribute("action", "disburse")
        .add_attribute("yield_distributor", info.sender)
        .add_attribute("recipient", recipient_addr)
        .add_attribute("amount", amount)
        .add_attribute("lp_yield_disbursed", updated_disbursed))
}

fn execute_admin_top_up(
    deps: DepsMut,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    require_admin(&info, &config)?;

    if amount.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    require_exact_single_fund(&info, &config.stablecoin_denom, amount)?;

    let topups = ADMIN_TOPUPS.load(deps.storage)?;
    let updated_topups = checked_add(topups, amount, "admin_topups + amount")?;
    ADMIN_TOPUPS.save(deps.storage, &updated_topups)?;

    Ok(Response::new()
        .add_attribute("action", "admin_top_up")
        .add_attribute("admin", info.sender)
        .add_attribute("amount", amount)
        .add_attribute("admin_topups", updated_topups))
}

fn execute_claim_protocol_revenue(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Option<Uint128>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    require_admin(&info, &config)?;
    require_no_funds(&info)?;

    // B15: Query PSPPool for state, require Settled or Closed
    use cosmwasm_std::{WasmQuery, QueryRequest};
    let psp_pool_addr = config.psp_pool.to_string();
    let pool_state_resp: PoolStateResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: psp_pool_addr,
        msg: to_json_binary(&PSPPoolQueryMsg::PoolState {})?,
    }))?;
    if pool_state_resp.state != "Settled" && pool_state_resp.state != "Closed" {
        return Err(ContractError::LpObligationsNotSettled {});
    }

    // Also check: total_disbursed >= total_received - gross_spread
    let fees_collected = FEES_COLLECTED.load(deps.storage)?;
    let lp_yield_disbursed = LP_YIELD_DISBURSED.load(deps.storage)?;
    let protocol_revenue_claimed = PROTOCOL_REVENUE_CLAIMED.load(deps.storage)?;
    let gross_spread = fees_collected.checked_sub(lp_yield_disbursed).map_err(|_| cosmwasm_std::StdError::generic_err("Accounting underflow: fees_collected - lp_yield_disbursed"))?;
    if lp_yield_disbursed < fees_collected.checked_sub(gross_spread).unwrap_or(Uint128::zero()) {
        return Err(ContractError::LpObligationsNotSettled {});
    }

    let claimable = query_protocol_revenue_claimable(deps.as_ref())?;
    if claimable.is_zero() {
        return Err(ContractError::NoProtocolRevenue {});
    }

    let claim_amount = amount.unwrap_or(claimable);
    if claim_amount.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }
    if claim_amount > claimable {
        return Err(ContractError::ClaimAmountExceedsClaimable {
            claimable,
            requested: claim_amount,
        });
    }

    let accounted_available = query_accounted_available(deps.as_ref())?;
    if claim_amount > accounted_available {
        return Err(ContractError::InsufficientAccountedBalance {
            available: accounted_available,
            required: claim_amount,
        });
    }

    let contract_balance = query_contract_balance(
        &deps.querier,
        env.contract.address.as_ref(),
        &config.stablecoin_denom,
    )?;
    if claim_amount > contract_balance {
        return Err(ContractError::InsufficientContractBalance {
            available: contract_balance,
            required: claim_amount,
        });
    }

    let claimed = PROTOCOL_REVENUE_CLAIMED.load(deps.storage)?;
    let updated_claimed = checked_add(
        claimed,
        claim_amount,
        "protocol_revenue_claimed + claim_amount",
    )?;
    PROTOCOL_REVENUE_CLAIMED.save(deps.storage, &updated_claimed)?;

    let send_msg = BankMsg::Send {
        to_address: config.admin.to_string(),
        amount: vec![Coin {
            denom: config.stablecoin_denom,
            amount: claim_amount,
        }],
    };

    Ok(Response::new()
        .add_message(send_msg)
        .add_attribute("action", "claim_protocol_revenue")
        .add_attribute("admin", info.sender)
        .add_attribute("amount", claim_amount)
        .add_attribute("protocol_revenue_claimed", updated_claimed))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&query_config(deps)?),
        QueryMsg::Balance {} => to_json_binary(&query_balance(deps, env)?),
        QueryMsg::Accounting {} => to_json_binary(&query_accounting(deps)?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        admin: config.admin.to_string(),
        credit_manager: config.credit_manager.to_string(),
        yield_distributor: config.yield_distributor.to_string(),
        stablecoin_denom: config.stablecoin_denom,
    })
}

fn query_balance(deps: Deps, env: Env) -> StdResult<BalanceResponse> {
    let config = CONFIG.load(deps.storage)?;
    let amount = query_contract_balance(
        &deps.querier,
        env.contract.address.as_ref(),
        &config.stablecoin_denom,
    )?;

    Ok(BalanceResponse {
        amount,
        denom: config.stablecoin_denom,
    })
}

fn query_accounting(deps: Deps) -> StdResult<AccountingResponse> {
    let fees_collected = FEES_COLLECTED.load(deps.storage)?;
    let admin_topups = ADMIN_TOPUPS.load(deps.storage)?;
    let lp_yield_disbursed = LP_YIELD_DISBURSED.load(deps.storage)?;
    let protocol_revenue_claimed = PROTOCOL_REVENUE_CLAIMED.load(deps.storage)?;

    let protocol_revenue_claimable = query_protocol_revenue_claimable(deps)?;
    let accounted_available_balance = query_accounted_available(deps)?;

    Ok(AccountingResponse {
        fees_collected,
        admin_topups,
        lp_yield_disbursed,
        protocol_revenue_claimed,
        protocol_revenue_claimable,
        accounted_available_balance,
    })
}

fn query_protocol_revenue_claimable(deps: Deps) -> StdResult<Uint128> {
    let fees_collected = FEES_COLLECTED.load(deps.storage)?;
    let lp_yield_disbursed = LP_YIELD_DISBURSED.load(deps.storage)?;
    let protocol_revenue_claimed = PROTOCOL_REVENUE_CLAIMED.load(deps.storage)?;

    let gross_spread = fees_collected
        .checked_sub(lp_yield_disbursed)
        .map_err(|_| cosmwasm_std::StdError::generic_err(
            ContractError::AccountingUnderflow {
                context: "fees_collected - lp_yield_disbursed".to_string(),
            }
            .to_string(),
        ))?;
    gross_spread
        .checked_sub(protocol_revenue_claimed)
        .map_err(|_| cosmwasm_std::StdError::generic_err(
            ContractError::AccountingUnderflow {
                context: "gross_spread - protocol_revenue_claimed".to_string(),
            }
            .to_string(),
        ))
}

fn query_accounted_available(deps: Deps) -> StdResult<Uint128> {
    let fees_collected = FEES_COLLECTED.load(deps.storage)?;
    let admin_topups = ADMIN_TOPUPS.load(deps.storage)?;
    let lp_yield_disbursed = LP_YIELD_DISBURSED.load(deps.storage)?;
    let protocol_revenue_claimed = PROTOCOL_REVENUE_CLAIMED.load(deps.storage)?;

    let inflows = checked_add(fees_collected, admin_topups, "fees_collected + admin_topups")
        .map_err(|e| cosmwasm_std::StdError::generic_err(e.to_string()))?;
    let outflows = checked_add(
        lp_yield_disbursed,
        protocol_revenue_claimed,
        "lp_yield_disbursed + protocol_revenue_claimed",
    )
    .map_err(|e| cosmwasm_std::StdError::generic_err(e.to_string()))?;

    inflows.checked_sub(outflows).map_err(|_| cosmwasm_std::StdError::generic_err(
        ContractError::AccountingUnderflow {
            context: "inflows - outflows (accounted_available)".to_string(),
        }
        .to_string(),
    ))
}

fn require_admin(info: &MessageInfo, config: &Config) -> Result<(), ContractError> {
    if info.sender != config.admin {
        return Err(ContractError::Unauthorized {
            caller: info.sender.to_string(),
            expected: config.admin.to_string(),
        });
    }
    Ok(())
}

fn require_credit_manager(info: &MessageInfo, config: &Config) -> Result<(), ContractError> {
    if info.sender != config.credit_manager {
        return Err(ContractError::CreditManagerOnly {});
    }
    Ok(())
}

fn require_yield_distributor(info: &MessageInfo, config: &Config) -> Result<(), ContractError> {
    if info.sender != config.yield_distributor {
        return Err(ContractError::YieldDistributorOnly {});
    }
    Ok(())
}

fn require_no_funds(info: &MessageInfo) -> Result<(), ContractError> {
    if !info.funds.is_empty() {
        return Err(ContractError::UnexpectedFunds {});
    }
    Ok(())
}

fn require_exact_single_fund(
    info: &MessageInfo,
    denom: &str,
    expected_amount: Uint128,
) -> Result<(), ContractError> {
    if info.funds.len() != 1 || info.funds[0].denom != denom || info.funds[0].amount != expected_amount {
        return Err(ContractError::InvalidFunds {
            denom: denom.to_string(),
        });
    }
    Ok(())
}

fn checked_add(
    a: Uint128,
    b: Uint128,
    operation: &str,
) -> Result<Uint128, ContractError> {
    a.checked_add(b)
        .map_err(|_| ContractError::ArithmeticOverflow {
            operation: operation.to_string(),
        })
}

fn query_contract_balance(
    querier: &QuerierWrapper,
    address: &str,
    denom: &str,
) -> StdResult<Uint128> {
    let balance = querier.query_balance(address, denom)?;
    Ok(balance.amount)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::new().add_attribute("action", "migrate"))
}

#[cfg(test)]
mod tests {
        use cosmwasm_std::{SystemResult, ContractResult, WasmQuery};
    use super::*;
    use crate::msg::InstantiateMsg;
    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env, MockApi};
    use cosmwasm_std::{coins, Addr};

    struct Actors {
        admin: String,
        credit_manager: String,
        yield_distributor: String,
        lp1: String,
        not_credit_manager: String,
        psp_pool: String,
    }

    fn instantiate_default(deps: DepsMut) -> Actors {
        let api = MockApi::default();
        let admin = api.addr_make("admin").to_string();
        let credit_manager = api.addr_make("credit_manager").to_string();
        let yield_distributor = api.addr_make("yield_distributor").to_string();
        let lp1 = api.addr_make("lp1").to_string();
        let not_credit_manager = api.addr_make("not_credit_manager").to_string();
        let psp_pool = api.addr_make("psp_pool").to_string();

        let msg = InstantiateMsg {
            admin: admin.clone(),
            credit_manager: credit_manager.clone(),
            yield_distributor: yield_distributor.clone(),
            stablecoin_denom: "uzig".to_string(),
            psp_pool: psp_pool.clone(),
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
        instantiate(deps, mock_env(), info, msg).unwrap();

        Actors {
            admin,
            credit_manager,
            yield_distributor,
            lp1,
            not_credit_manager,
            psp_pool,
        }
    }

    #[test]
    fn only_credit_manager_can_receive_from() {
        let mut deps = mock_dependencies();
        let actors = instantiate_default(deps.as_mut());

        let unauthorized_info = message_info(
            &Addr::unchecked(actors.not_credit_manager),
            &coins(100, "uzig"),
        );
        let err = execute(
            deps.as_mut(),
            mock_env(),
            unauthorized_info,
            ExecuteMsg::ReceiveFrom {
                amount: Uint128::new(100),
            },
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::CreditManagerOnly {}));
    }

    #[test]
    fn receive_disburse_and_claim_update_accounting() {
        let mut deps = mock_dependencies();
        let actors = instantiate_default(deps.as_mut());
        let psp_pool_addr = actors.psp_pool.clone();
        deps.querier.update_wasm(move |query| match query {
            WasmQuery::Smart { contract_addr, .. } if *contract_addr == psp_pool_addr => {
                let resp = super::PoolStateResponse {
                    state: "Settled".to_string(),
                    paused: false,
                };
                let bin = cosmwasm_std::to_json_binary(&resp).unwrap();
                SystemResult::Ok(ContractResult::Ok(bin))
            }
            _ => SystemResult::Err(cosmwasm_std::SystemError::UnsupportedRequest { kind: "unsupported".to_string() }),
        });
        let env = mock_env();

        // Simulate contract holding funds for send checks in this unit test environment.
        deps.querier
            .bank
            .update_balance(env.contract.address.to_string(), coins(10_000, "uzig"));

        let credit_manager_info =
            message_info(&Addr::unchecked(actors.credit_manager), &coins(500, "uzig"));
        execute(
            deps.as_mut(),
            env.clone(),
            credit_manager_info,
            ExecuteMsg::ReceiveFrom {
                amount: Uint128::new(500),
            },
        )
        .unwrap();

        let admin_top_up_info = message_info(&Addr::unchecked(actors.admin.clone()), &coins(300, "uzig"));
        execute(
            deps.as_mut(),
            env.clone(),
            admin_top_up_info,
            ExecuteMsg::AdminTopUp {
                amount: Uint128::new(300),
            },
        )
        .unwrap();

        let distributor_info =
            message_info(&Addr::unchecked(actors.yield_distributor), &[]);
        execute(
            deps.as_mut(),
            env.clone(),
            distributor_info,
            ExecuteMsg::Disburse {
                recipient: actors.lp1,
                amount: Uint128::new(400),
            },
        )
        .unwrap();

        let claimable = query_protocol_revenue_claimable(deps.as_ref()).unwrap();
        assert_eq!(claimable, Uint128::new(100));

        let admin_claim_info = message_info(&Addr::unchecked(actors.admin), &[]);
        execute(
            deps.as_mut(),
            env,
            admin_claim_info,
            ExecuteMsg::ClaimProtocolRevenue { amount: None },
        )
        .unwrap();

        let accounting = query_accounting(deps.as_ref()).unwrap();
        assert_eq!(accounting.fees_collected, Uint128::new(500));
        assert_eq!(accounting.admin_topups, Uint128::new(300));
        assert_eq!(accounting.lp_yield_disbursed, Uint128::new(400));
        assert_eq!(accounting.protocol_revenue_claimed, Uint128::new(100));
        assert_eq!(accounting.protocol_revenue_claimable, Uint128::zero());
        assert_eq!(accounting.accounted_available_balance, Uint128::new(300));
    }

    #[test]
    fn cannot_claim_more_than_protocol_revenue() {
        let mut deps = mock_dependencies();
        let actors = instantiate_default(deps.as_mut());
        let psp_pool_addr = actors.psp_pool.clone();
        deps.querier.update_wasm(move |query| match query {
            WasmQuery::Smart { contract_addr, .. } if *contract_addr == psp_pool_addr => {
                let resp = super::PoolStateResponse {
                    state: "Settled".to_string(),
                    paused: false,
                };
                let bin = cosmwasm_std::to_json_binary(&resp).unwrap();
                SystemResult::Ok(ContractResult::Ok(bin))
            }
            _ => SystemResult::Err(cosmwasm_std::SystemError::UnsupportedRequest { kind: "unsupported".to_string() }),
        });
        let env = mock_env();

        deps.querier
            .bank
            .update_balance(env.contract.address.to_string(), coins(1_000, "uzig"));

        let credit_manager_info =
            message_info(&Addr::unchecked(actors.credit_manager), &coins(200, "uzig"));
        execute(
            deps.as_mut(),
            env.clone(),
            credit_manager_info,
            ExecuteMsg::ReceiveFrom {
                amount: Uint128::new(200),
            },
        )
        .unwrap();

        let distributor_info =
            message_info(&Addr::unchecked(actors.yield_distributor), &[]);
        execute(
            deps.as_mut(),
            env.clone(),
            distributor_info,
            ExecuteMsg::Disburse {
                recipient: actors.lp1,
                amount: Uint128::new(150),
            },
        )
        .unwrap();

        let admin_claim_info = message_info(&Addr::unchecked(actors.admin), &[]);
        let err = execute(
            deps.as_mut(),
            env,
            admin_claim_info,
            ExecuteMsg::ClaimProtocolRevenue {
                amount: Some(Uint128::new(100)),
            },
        )
        .unwrap_err();

        assert!(matches!(
            err,
            ContractError::ClaimAmountExceedsClaimable { .. }
        ));
    }
}
