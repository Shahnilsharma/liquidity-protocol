use defa_types::addr_validate_bypass::addr_validate_bypass;
use cosmwasm_std::{
    entry_point, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Order,
    Reply, Response, StdResult, SubMsg, SubMsgResult, WasmMsg,
};
use cosmwasm_schema::cw_serde;
use cw2::set_contract_version;
use cw_storage_plus::Bound;

use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, PoolRecordResponse,
    PoolResponse, PoolsResponse, QueryMsg,
};
use defa_yield_reserve::msg::InstantiateMsg as YieldReserveInstantiateMsg;
use crate::state::{
    Config, PendingPoolCreation, PoolRecord, CONFIG, CONTRACT_NAME, CONTRACT_VERSION,
    NEXT_POOL_ID, PENDING_POOL_CREATION, POOLS, REPLY_ID_CREDIT_MANAGER,
    REPLY_ID_PSP_POOL, REPLY_ID_YIELD_DISTRIBUTOR, REPLY_ID_YIELD_RESERVE,
};

const DEFAULT_PAGE_LIMIT: u32 = 20;
const MAX_PAGE_LIMIT: u32 = 50;

#[cw_serde]
enum PspPoolExecuteMsg {
    SetCreditManager { credit_manager: String },
}

#[cw_serde]
enum CreditManagerExecuteMsg {
    UpdateWiring {
        psp_pool: Option<String>,
        yield_reserve: Option<String>,
        max_queue_wait_days: Option<u64>,
    },
}

#[cw_serde]
enum YieldDistributorExecuteMsg {
    UpdateWiring {
        psp_pool: Option<String>,
        yield_reserve: Option<String>,
    },
}

#[cw_serde]
enum YieldReserveExecuteMsg {
    UpdateWiring {
        credit_manager: Option<String>,
        yield_distributor: Option<String>,
    },
}

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
    CONFIG.save(
        deps.storage,
        &Config {
            admin: admin.clone(),
            psp_pool_code_id: msg.psp_pool_code_id,
            credit_manager_code_id: msg.credit_manager_code_id,
            yield_distributor_code_id: msg.yield_distributor_code_id,
            yield_reserve_code_id: msg.yield_reserve_code_id,
        },
    )?;
    NEXT_POOL_ID.save(deps.storage, &0u64)?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("admin", admin)
        .add_attribute("psp_pool_code_id", msg.psp_pool_code_id.to_string())
        .add_attribute("credit_manager_code_id", msg.credit_manager_code_id.to_string())
        .add_attribute("yield_distributor_code_id", msg.yield_distributor_code_id.to_string())
        .add_attribute("yield_reserve_code_id", msg.yield_reserve_code_id.to_string()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::SetCodeIds {
            psp_pool_code_id,
            credit_manager_code_id,
            yield_distributor_code_id,
            yield_reserve_code_id,
        } => execute_set_code_ids(
            deps,
            info,
            psp_pool_code_id,
            credit_manager_code_id,
            yield_distributor_code_id,
            yield_reserve_code_id,
        ),
        ExecuteMsg::CreatePool {
            label_prefix,
            psp_pool_init_msg,
            credit_manager_init_msg,
            yield_distributor_init_msg,
            yield_reserve_init_msg,
        } => execute_create_pool(
            deps,
            env,
            info,
            label_prefix,
            psp_pool_init_msg,
            credit_manager_init_msg,
            yield_distributor_init_msg,
            yield_reserve_init_msg,
        ),
    }
}

fn execute_set_code_ids(
    deps: DepsMut,
    info: MessageInfo,
    psp_pool_code_id: u64,
    credit_manager_code_id: u64,
    yield_distributor_code_id: u64,
    yield_reserve_code_id: u64,
) -> Result<Response, ContractError> {
    CONFIG.update(deps.storage, |mut config| -> Result<_, ContractError> {
        if info.sender != config.admin {
            return Err(ContractError::Unauthorized {
                caller: info.sender.to_string(),
                expected: config.admin.to_string(),
            });
        }
        config.psp_pool_code_id = psp_pool_code_id;
        config.credit_manager_code_id = credit_manager_code_id;
        config.yield_distributor_code_id = yield_distributor_code_id;
        config.yield_reserve_code_id = yield_reserve_code_id;
        Ok(config)
    })?;

    Ok(Response::new()
        .add_attribute("action", "set_code_ids")
        .add_attribute("admin", info.sender)
        .add_attribute("psp_pool_code_id", psp_pool_code_id.to_string())
        .add_attribute("credit_manager_code_id", credit_manager_code_id.to_string())
        .add_attribute("yield_distributor_code_id", yield_distributor_code_id.to_string())
        .add_attribute("yield_reserve_code_id", yield_reserve_code_id.to_string()))
}

#[allow(clippy::too_many_arguments)]
fn execute_create_pool(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    label_prefix: String,
    psp_pool_init_msg: defa_psp_pool::msg::InstantiateMsg,
    credit_manager_init_msg: defa_credit_manager::msg::InstantiateMsg,
    yield_distributor_init_msg: defa_yield_distributor::msg::InstantiateMsg,
    yield_reserve_init_msg: defa_yield_reserve::msg::InstantiateMsg,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.admin {
        return Err(ContractError::Unauthorized {
            caller: info.sender.to_string(),
            expected: config.admin.to_string(),
        });
    }

    if label_prefix.trim().is_empty() {
        return Err(ContractError::InvalidLabelPrefix {});
    }

    if PENDING_POOL_CREATION.may_load(deps.storage)?.is_some() {
        return Err(ContractError::PendingCreationInProgress {});
    }


    let pool_id = NEXT_POOL_ID.load(deps.storage)?;
    let next_pool_id = pool_id
        .checked_add(1)
        .ok_or_else(|| ContractError::ArithmeticOverflow {
            operation: "next_pool_id".to_string(),
        })?;
    NEXT_POOL_ID.save(deps.storage, &next_pool_id)?;

    let psp_pool_init_msg_bin = cosmwasm_std::to_json_binary(&psp_pool_init_msg)?;
    let credit_manager_init_msg_bin = cosmwasm_std::to_json_binary(&credit_manager_init_msg)?;
    let yield_distributor_init_msg_bin = cosmwasm_std::to_json_binary(&yield_distributor_init_msg)?;
    let yield_reserve_init_msg_bin = cosmwasm_std::to_json_binary(&yield_reserve_init_msg)?;

    PENDING_POOL_CREATION.save(
        deps.storage,
        &PendingPoolCreation {
            pool_id,
            label_prefix: label_prefix.clone(),
            created_at: env.block.time.seconds(),
            psp_pool_init_msg: psp_pool_init_msg_bin.clone(),
            credit_manager_init_msg: credit_manager_init_msg_bin.clone(),
            yield_distributor_init_msg: yield_distributor_init_msg_bin.clone(),
            yield_reserve_init_msg: yield_reserve_init_msg_bin.clone(),
            psp_pool: None,
            credit_manager: None,
            yield_distributor: None,
        },
    )?;

    let instantiate_psp_pool = WasmMsg::Instantiate {
        admin: Some(config.admin.to_string()),
        code_id: config.psp_pool_code_id,
        msg: psp_pool_init_msg_bin,
        funds: vec![],
        label: format!("{}-psp-pool-{}", label_prefix, pool_id),
    };

    Ok(Response::new()
        .add_submessage(SubMsg::reply_on_success(
            instantiate_psp_pool,
            REPLY_ID_PSP_POOL,
        ))
        .add_attribute("action", "create_pool")
        .add_attribute("pool_id", pool_id.to_string())
        .add_attribute("initiator", info.sender))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(
    deps: DepsMut,
    _env: Env,
    reply: Reply,
) -> Result<Response, ContractError> {
    let mut pending = PENDING_POOL_CREATION
        .may_load(deps.storage)?
        .ok_or(ContractError::PendingCreationNotFound {})?;
    let config = CONFIG.load(deps.storage)?;

    match reply.id {
        REPLY_ID_PSP_POOL => {
            let psp_pool_addr = deps.api.addr_validate(&parse_reply_address(&reply)?)?;
            pending.psp_pool = Some(psp_pool_addr.clone());

            // Inject psp_pool address into yield_reserve_init_msg
            let mut yield_reserve_init: YieldReserveInstantiateMsg = cosmwasm_std::from_json(&pending.yield_reserve_init_msg)?;
            yield_reserve_init.psp_pool = psp_pool_addr.to_string();
            let updated_yield_reserve_init_msg = cosmwasm_std::to_json_binary(&yield_reserve_init)?;
            pending.yield_reserve_init_msg = updated_yield_reserve_init_msg;

            PENDING_POOL_CREATION.save(deps.storage, &pending)?;

            let instantiate_credit_manager = WasmMsg::Instantiate {
                admin: Some(config.admin.to_string()),
                code_id: config.credit_manager_code_id,
                msg: pending.credit_manager_init_msg.clone(),
                funds: vec![],
                label: format!(
                    "{}-credit-manager-{}",
                    pending.label_prefix, pending.pool_id
                ),
            };

            Ok(Response::new()
                .add_submessage(SubMsg::reply_on_success(
                    instantiate_credit_manager,
                    REPLY_ID_CREDIT_MANAGER,
                ))
                .add_attribute("action", "reply_psp_pool")
                .add_attribute("pool_id", pending.pool_id.to_string())
                .add_attribute("psp_pool", psp_pool_addr))
        }
        REPLY_ID_CREDIT_MANAGER => {
            let credit_manager_addr = deps.api.addr_validate(&parse_reply_address(&reply)?)?;
            pending.credit_manager = Some(credit_manager_addr.clone());
            PENDING_POOL_CREATION.save(deps.storage, &pending)?;

            let instantiate_yield_distributor = WasmMsg::Instantiate {
                admin: Some(config.admin.to_string()),
                code_id: config.yield_distributor_code_id,
                msg: pending.yield_distributor_init_msg.clone(),
                funds: vec![],
                label: format!(
                    "{}-yield-distributor-{}",
                    pending.label_prefix, pending.pool_id
                ),
            };

            Ok(Response::new()
                .add_submessage(SubMsg::reply_on_success(
                    instantiate_yield_distributor,
                    REPLY_ID_YIELD_DISTRIBUTOR,
                ))
                .add_attribute("action", "reply_credit_manager")
                .add_attribute("pool_id", pending.pool_id.to_string())
                .add_attribute("credit_manager", credit_manager_addr))
        }
        REPLY_ID_YIELD_DISTRIBUTOR => {
            let yield_distributor_addr = deps.api.addr_validate(&parse_reply_address(&reply)?)?;
            pending.yield_distributor = Some(yield_distributor_addr.clone());
            PENDING_POOL_CREATION.save(deps.storage, &pending)?;

            let instantiate_yield_reserve = WasmMsg::Instantiate {
                admin: Some(config.admin.to_string()),
                code_id: config.yield_reserve_code_id,
                msg: pending.yield_reserve_init_msg.clone(),
                funds: vec![],
                label: format!(
                    "{}-yield-reserve-{}",
                    pending.label_prefix, pending.pool_id
                ),
            };

            Ok(Response::new()
                .add_submessage(SubMsg::reply_on_success(
                    instantiate_yield_reserve,
                    REPLY_ID_YIELD_RESERVE,
                ))
                .add_attribute("action", "reply_yield_distributor")
                .add_attribute("pool_id", pending.pool_id.to_string())
                .add_attribute("yield_distributor", yield_distributor_addr))
        }
        REPLY_ID_YIELD_RESERVE => {
            let yield_reserve_addr = deps.api.addr_validate(&parse_reply_address(&reply)?)?;
            let psp_pool = pending.psp_pool.take().ok_or(ContractError::IncompletePendingCreation {})?;
            let credit_manager = pending
                .credit_manager
                .take()
                .ok_or(ContractError::IncompletePendingCreation {})?;
            let yield_distributor = pending
                .yield_distributor
                .take()
                .ok_or(ContractError::IncompletePendingCreation {})?;

            let record = PoolRecord {
                pool_id: pending.pool_id,
                psp_pool: psp_pool.clone(),
                credit_manager: credit_manager.clone(),
                yield_distributor: yield_distributor.clone(),
                yield_reserve: yield_reserve_addr.clone(),
                created_at: pending.created_at,
            };

            POOLS.save(deps.storage, pending.pool_id, &record)?;
            PENDING_POOL_CREATION.remove(deps.storage);

            let wire_psp_pool = WasmMsg::Execute {
                contract_addr: psp_pool.to_string(),
                msg: to_json_binary(&PspPoolExecuteMsg::SetCreditManager {
                    credit_manager: credit_manager.to_string(),
                })?,
                funds: vec![],
            };

            let wire_credit_manager = WasmMsg::Execute {
                contract_addr: credit_manager.to_string(),
                msg: to_json_binary(&CreditManagerExecuteMsg::UpdateWiring {
                    psp_pool: Some(psp_pool.to_string()),
                    yield_reserve: Some(yield_reserve_addr.to_string()),
                    max_queue_wait_days: None,
                })?,
                funds: vec![],
            };

            let wire_yield_distributor = WasmMsg::Execute {
                contract_addr: yield_distributor.to_string(),
                msg: to_json_binary(&YieldDistributorExecuteMsg::UpdateWiring {
                    psp_pool: Some(psp_pool.to_string()),
                    yield_reserve: Some(yield_reserve_addr.to_string()),
                })?,
                funds: vec![],
            };

            let wire_yield_reserve = WasmMsg::Execute {
                contract_addr: yield_reserve_addr.to_string(),
                msg: to_json_binary(&YieldReserveExecuteMsg::UpdateWiring {
                    credit_manager: Some(credit_manager.to_string()),
                    yield_distributor: Some(yield_distributor.to_string()),
                })?,
                funds: vec![],
            };

            Ok(Response::new()
                .add_message(wire_psp_pool)
                .add_message(wire_credit_manager)
                .add_message(wire_yield_distributor)
                .add_message(wire_yield_reserve)
                .add_attribute("action", "pool_created")
                .add_attribute("pool_id", record.pool_id.to_string())
                .add_attribute("psp_pool", psp_pool)
                .add_attribute("credit_manager", credit_manager)
                .add_attribute("yield_distributor", yield_distributor)
                .add_attribute("yield_reserve", yield_reserve_addr))
        }
        other => Err(ContractError::InvalidReplyId { reply_id: other }),
    }
}

fn parse_reply_address(reply: &Reply) -> Result<String, ContractError> {
    match &reply.result {
        SubMsgResult::Err(reason) => Err(ContractError::SubMsgFailed {
            reason: reason.clone(),
        }),
        SubMsgResult::Ok(response) => {
            for event in &response.events {
                for attr in &event.attributes {
                    if attr.key == "_contract_address" {
                        return Ok(attr.value.clone());
                    }
                }
            }
            Err(ContractError::MissingReplyAddress {})
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&query_config(deps)?),
        QueryMsg::Pool { pool_id } => to_json_binary(&query_pool(deps, pool_id)?),
        QueryMsg::Pools { start_after, limit } => {
            to_json_binary(&query_pools(deps, start_after, limit)?)
        }
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        admin: config.admin.to_string(),
        psp_pool_code_id: config.psp_pool_code_id,
        credit_manager_code_id: config.credit_manager_code_id,
        yield_distributor_code_id: config.yield_distributor_code_id,
        yield_reserve_code_id: config.yield_reserve_code_id,
    })
}

fn query_pool(deps: Deps, pool_id: u64) -> StdResult<PoolResponse> {
    let pool = POOLS.may_load(deps.storage, pool_id)?;
    Ok(PoolResponse {
        pool: pool.map(to_pool_record_response),
    })
}

fn query_pools(
    deps: Deps,
    start_after: Option<u64>,
    limit: Option<u32>,
) -> StdResult<PoolsResponse> {
    let page_size = match limit {
        Some(0) => return Err(cosmwasm_std::StdError::generic_err(
            ContractError::InvalidPaginationLimit {}.to_string(),
        )),
        Some(value) => value.min(MAX_PAGE_LIMIT) as usize,
        None => DEFAULT_PAGE_LIMIT as usize,
    };

    let start = start_after.map(Bound::exclusive);
    let pools: StdResult<Vec<PoolRecordResponse>> = POOLS
        .range(deps.storage, start, None, Order::Ascending)
        .take(page_size)
        .map(|item| item.map(|(_, record)| to_pool_record_response(record)))
        .collect();

    Ok(PoolsResponse { pools: pools? })
}

fn to_pool_record_response(record: PoolRecord) -> PoolRecordResponse {
    PoolRecordResponse {
        pool_id: record.pool_id,
        psp_pool: record.psp_pool.to_string(),
        credit_manager: record.credit_manager.to_string(),
        yield_distributor: record.yield_distributor.to_string(),
        yield_reserve: record.yield_reserve.to_string(),
        created_at: record.created_at,
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::new().add_attribute("action", "migrate"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{message_info, mock_dependencies, mock_env, MockApi};
    use cosmwasm_std::{from_json, Binary, Event, Reply, SubMsgResponse};

    fn addr(label: &str) -> String {
        MockApi::default().addr_make(label).to_string()
    }

    fn instantiate_default(deps: DepsMut) -> String {
        let admin = addr("admin");
        let msg = InstantiateMsg {
            admin: admin.clone(),
            psp_pool_code_id: 11,
            credit_manager_code_id: 12,
            yield_distributor_code_id: 13,
            yield_reserve_code_id: 14,
        };
        let info = message_info(&MockApi::default().addr_make("creator"), &[]);
        instantiate(deps, mock_env(), info, msg).unwrap();
        admin
    }

    fn ok_reply(id: u64, contract_address: &str) -> Reply {
        #[allow(deprecated)]
        Reply {
            id,
            payload: Binary::default(),
            gas_used: 0,
            result: SubMsgResult::Ok(SubMsgResponse {
                events: vec![
                    Event::new("instantiate")
                        .add_attribute("_contract_address", contract_address),
                ],
                data: None,
                msg_responses: vec![],
            }),
        }
    }

    #[test]
    fn only_admin_can_set_code_ids() {
        let mut deps = mock_dependencies();
        let admin = instantiate_default(deps.as_mut());

        let not_admin = message_info(&MockApi::default().addr_make("not_admin"), &[]);
        let err = execute(
            deps.as_mut(),
            mock_env(),
            not_admin,
            ExecuteMsg::SetCodeIds {
                psp_pool_code_id: 101,
                credit_manager_code_id: 102,
                yield_distributor_code_id: 103,
                yield_reserve_code_id: 104,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized { .. }));
            if let ContractError::Unauthorized { .. } = err {
                // pass
            } else {
                panic!("Expected Unauthorized error");
            }

        let admin_info = message_info(&MockApi::default().addr_make("admin"), &[]);
        execute(
            deps.as_mut(),
            mock_env(),
            admin_info,
            ExecuteMsg::SetCodeIds {
                psp_pool_code_id: 201,
                credit_manager_code_id: 202,
                yield_distributor_code_id: 203,
                yield_reserve_code_id: 204,
            },
        )
        .unwrap();

        let cfg_bin = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
        let cfg: ConfigResponse = from_json(&cfg_bin).unwrap();
        assert_eq!(cfg.admin, admin);
        assert_eq!(cfg.psp_pool_code_id, 201);
        assert_eq!(cfg.credit_manager_code_id, 202);
        assert_eq!(cfg.yield_distributor_code_id, 203);
        assert_eq!(cfg.yield_reserve_code_id, 204);
    }

    #[test]
    fn create_pool_reply_chain_registers_pool() {
        let mut deps = mock_dependencies();
        let admin = instantiate_default(deps.as_mut());
        let env = mock_env();

        let create_res = execute(
            deps.as_mut(),
            env.clone(),
            message_info(&MockApi::default().addr_make("admin"), &[]),
            ExecuteMsg::CreatePool {
                label_prefix: "facility-a".to_string(),
                psp_pool_init_msg: Binary::from(br#"{}"#.as_slice()),
                credit_manager_init_msg: Binary::from(br#"{}"#.as_slice()),
                yield_distributor_init_msg: Binary::from(br#"{}"#.as_slice()),
                yield_reserve_init_msg: Binary::from(br#"{}"#.as_slice()),
            },
        )
        .unwrap();

        assert_eq!(create_res.messages.len(), 1);
        match &create_res.messages[0].msg {
            cosmwasm_std::CosmosMsg::Wasm(WasmMsg::Instantiate { code_id, .. }) => {
                assert_eq!(*code_id, 11);
            }
            _ => panic!("expected instantiate submessage"),
        }

        let psp_pool = addr("psp_pool_0");
        let credit_manager = addr("credit_manager_0");
        let yield_distributor = addr("yield_distributor_0");
        let yield_reserve = addr("yield_reserve_0");

        let r1 = reply(deps.as_mut(), env.clone(), ok_reply(REPLY_ID_PSP_POOL, &psp_pool)).unwrap();
        assert_eq!(r1.messages.len(), 1);

        let r2 = reply(
            deps.as_mut(),
            env.clone(),
            ok_reply(REPLY_ID_CREDIT_MANAGER, &credit_manager),
        )
        .unwrap();
        assert_eq!(r2.messages.len(), 1);

        let r3 = reply(
            deps.as_mut(),
            env.clone(),
            ok_reply(REPLY_ID_YIELD_DISTRIBUTOR, &yield_distributor),
        )
        .unwrap();
        assert_eq!(r3.messages.len(), 1);

        let r4 = reply(
            deps.as_mut(),
            env,
            ok_reply(REPLY_ID_YIELD_RESERVE, &yield_reserve),
        )
        .unwrap();
        assert_eq!(r4.messages.len(), 4);

        let pool_bin = query(deps.as_ref(), mock_env(), QueryMsg::Pool { pool_id: 0 }).unwrap();
        let pool: PoolResponse = from_json(&pool_bin).unwrap();
        let pool = pool.pool.expect("pool should exist");
        assert_eq!(pool.pool_id, 0);
        assert_eq!(pool.psp_pool, psp_pool);
        assert_eq!(pool.credit_manager, credit_manager);
        assert_eq!(pool.yield_distributor, yield_distributor);
        assert_eq!(pool.yield_reserve, yield_reserve);

        let pools_bin = query(
            deps.as_ref(),
            mock_env(),
            QueryMsg::Pools {
                start_after: None,
                limit: Some(10),
            },
        )
        .unwrap();
        let pools: PoolsResponse = from_json(&pools_bin).unwrap();
        assert_eq!(pools.pools.len(), 1);

        let cfg_bin = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
        let cfg: ConfigResponse = from_json(&cfg_bin).unwrap();
        assert_eq!(cfg.admin, admin);
    }

    #[test]
    fn create_pool_rejects_empty_label_prefix() {
        let mut deps = mock_dependencies();
        instantiate_default(deps.as_mut());

        let err = execute(
            deps.as_mut(),
            mock_env(),
            message_info(&MockApi::default().addr_make("admin"), &[]),
            ExecuteMsg::CreatePool {
                label_prefix: "   ".to_string(),
                psp_pool_init_msg: Binary::from(br#"{}"#.as_slice()),
                credit_manager_init_msg: Binary::from(br#"{}"#.as_slice()),
                yield_distributor_init_msg: Binary::from(br#"{}"#.as_slice()),
                yield_reserve_init_msg: Binary::from(br#"{}"#.as_slice()),
            },
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::InvalidLabelPrefix {}));
    }
}
