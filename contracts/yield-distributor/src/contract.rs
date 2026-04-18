#[cfg(feature = "test-addr-bypass")]
use defa_types::addr_validate_bypass::addr_validate_bypass;
use std::collections::BTreeMap;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    entry_point, to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response,
    StdResult, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use defa_types::{APY_DAYS_BASIS, BPS_DENOMINATOR, SECONDS_PER_DAY};

use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, CycleStatusResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
};
// Import BalanceResponse and QueryMsg from yield-reserve for cross-contract query
use defa_yield_reserve::msg::{BalanceResponse as YReserveBalanceResponse, QueryMsg as YReserveQueryMsg};
use crate::state::{
    ActiveDisbursement, Config, CycleState, CONFIG, CONTRACT_NAME,
    CONTRACT_VERSION, CYCLE_STATE,
};

const DEFAULT_BATCH_LIMIT: u32 = 50;
const MAX_BATCH_LIMIT: u32 = 150;

#[cw_serde]
enum YieldReserveExecuteMsg {
    Disburse { recipient: String, amount: Uint128 },
}

#[cw_serde]
enum PSPPoolQueryMsg {
    VaultInfo {},
    UserInfo { address: String },
    LpHolders {
        start_after: Option<String>,
        limit: Option<u32>,
    },
}

#[cw_serde]
struct PSPPoolVaultInfoResponse {
    total_deposited: Uint128,
    total_lp_supply: Uint128,
}

#[cw_serde]
struct PSPPoolUserInfoResponse {
    lp_balance: Uint128,
    deposit_timestamp: u64,
}

#[cw_serde]
struct PSPPoolLpHoldersResponse {
    holders: Vec<String>,
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    if msg.cycle_days == 0 {
        return Err(ContractError::InvalidCycleDays {});
    }
    if msg.investor_apy_bps == 0 {
        return Err(ContractError::InvalidInvestorApy {});
    }

    #[cfg(feature = "test-addr-bypass")]
    let admin = cosmwasm_std::Addr::unchecked(addr_validate_bypass(&msg.admin).map_err(|e| cosmwasm_std::StdError::generic_err(e))?);
    #[cfg(not(feature = "test-addr-bypass"))]
    let admin = deps.api.addr_validate(&msg.admin)?;

    #[cfg(feature = "test-addr-bypass")]
    let psp_pool = cosmwasm_std::Addr::unchecked(addr_validate_bypass(&msg.psp_pool).map_err(|e| cosmwasm_std::StdError::generic_err(e))?);
    #[cfg(not(feature = "test-addr-bypass"))]
    let psp_pool = deps.api.addr_validate(&msg.psp_pool)?;

    #[cfg(feature = "test-addr-bypass")]
    let yield_reserve = cosmwasm_std::Addr::unchecked(addr_validate_bypass(&msg.yield_reserve).map_err(|e| cosmwasm_std::StdError::generic_err(e))?);
    #[cfg(not(feature = "test-addr-bypass"))]
    let yield_reserve = deps.api.addr_validate(&msg.yield_reserve)?;
    let cycle_anchor_time = msg.cycle_anchor_time.unwrap_or(env.block.time.seconds());

    CONFIG.save(
        deps.storage,
        &Config {
            admin: admin.clone(),
            psp_pool: psp_pool.clone(),
            yield_reserve: yield_reserve.clone(),
            investor_apy_bps: msg.investor_apy_bps,
            cycle_days: msg.cycle_days,
            cycle_anchor_time,
        },
    )?;

    CYCLE_STATE.save(
        deps.storage,
        &CycleState {
            completed_cycles: 0,
            last_disbursed_at: None,
            active_disbursement: None,
        },
    )?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("admin", admin)
        .add_attribute("psp_pool", psp_pool)
        .add_attribute("yield_reserve", yield_reserve)
        .add_attribute("investor_apy_bps", msg.investor_apy_bps.to_string())
        .add_attribute("cycle_days", msg.cycle_days.to_string())
        .add_attribute("cycle_anchor_time", cycle_anchor_time.to_string()))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateWiring {
            psp_pool,
            yield_reserve,
        } => execute_update_wiring(deps, info, psp_pool, yield_reserve),
        ExecuteMsg::DisburseYield { limit } => {
            execute_disburse_yield(deps, env, info, limit)
        }
    }
}

fn execute_update_wiring(
    deps: DepsMut,
    info: MessageInfo,
    psp_pool: Option<String>,
    yield_reserve: Option<String>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    require_admin(&info, &config)?;
    require_no_funds(&info)?;

    let mut response = Response::new()
        .add_attribute("action", "update_wiring")
        .add_attribute("admin", info.sender.to_string());

    if let Some(psp_pool_addr) = psp_pool {
        let validated = deps.api.addr_validate(&psp_pool_addr)?;
        config.psp_pool = validated.clone();
        response = response.add_attribute("psp_pool", validated);
    }

    if let Some(yield_reserve_addr) = yield_reserve {
        let validated = deps.api.addr_validate(&yield_reserve_addr)?;
        config.yield_reserve = validated.clone();
        response = response.add_attribute("yield_reserve", validated);
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(response)
}

fn execute_disburse_yield(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    limit: Option<u32>,
) -> Result<Response, ContractError> {
    require_no_funds(&info)?;

    let config = CONFIG.load(deps.storage)?;
    let mut state = CYCLE_STATE.load(deps.storage)?;
    let now = env.block.time.seconds();

    if state.active_disbursement.is_none() {
        let next_cycle_time = compute_next_cycle_time(&config, state.completed_cycles)?;
        if now < next_cycle_time {
            return Err(ContractError::CycleNotReady {
                next_cycle_time,
                now,
            });
        }

        let vault = query_psp_vault_info(deps.as_ref(), &config)?;
        if vault.total_lp_supply.is_zero() {
            return Err(ContractError::NoLpSupply {});
        }

        let cycle_amount = calculate_cycle_amount(
            vault.total_deposited,
            config.investor_apy_bps,
            config.cycle_days,
        )?;

        // B14: Pre-check YieldReserve balance before mutating state
        if !cycle_amount.is_zero() {
            use cosmwasm_std::{WasmQuery, QueryRequest};
            let balance_resp: YReserveBalanceResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: config.yield_reserve.to_string(),
                msg: to_json_binary(&YReserveQueryMsg::Balance {})?,
            }))?;
            if balance_resp.amount < cycle_amount {
                return Err(crate::error::ContractError::InsufficientYieldReserve {
                    required: cycle_amount,
                    available: balance_resp.amount,
                });
            }
        }

        if cycle_amount.is_zero() {
            state.completed_cycles = checked_add_u64(
                state.completed_cycles,
                1,
                "completed_cycles + 1",
            )?;
            state.last_disbursed_at = Some(now);
            state.active_disbursement = None;
            CYCLE_STATE.save(deps.storage, &state)?;

            return Ok(Response::new()
                .add_attribute("action", "disburse_yield_noop")
                .add_attribute("triggered_by", info.sender)
                .add_attribute("cycle_amount", cycle_amount)
                .add_attribute("completed_cycles", state.completed_cycles.to_string()));
        }

        state.active_disbursement = Some(ActiveDisbursement {
            total_lp_supply: vault.total_lp_supply,
            cycle_amount,
            distributed_amount: Uint128::zero(),
            next_start_after: None,
        });
    }

    let batch_limit = normalize_limit(limit)?;
    let fetch_limit = checked_add_u32(batch_limit, 1, "batch_limit + 1")?;
    let mut active = state
        .active_disbursement
        .clone()
        .ok_or(ContractError::ArithmeticOverflow {
            operation: "missing active_disbursement".to_string(),
        })?;

    let holders_response: PSPPoolLpHoldersResponse = deps.querier.query_wasm_smart(
        config.psp_pool.to_string(),
        &PSPPoolQueryMsg::LpHolders {
            start_after: active.next_start_after.clone(),
            limit: Some(fetch_limit),
        },
    )?;

    if holders_response.holders.is_empty() {
        return Err(ContractError::NoIndexedLpHolders {});
    }

    let mut has_more = false;
    let mut holders_batch = holders_response.holders;
    if holders_batch.len() > batch_limit as usize {
        holders_batch.truncate(batch_limit as usize);
        has_more = true;
    }

    let mut payouts: BTreeMap<String, Uint128> = BTreeMap::new();
    let mut batch_total = Uint128::zero();

    for holder in &holders_batch {
        let user_info: PSPPoolUserInfoResponse = deps.querier.query_wasm_smart(
            config.psp_pool.to_string(),
            &PSPPoolQueryMsg::UserInfo {
                address: holder.clone(),
            },
        )?;

        if user_info.lp_balance.is_zero() {
            continue;
        }

        // Calculate eligible days for this user
        let deposit_secs = user_info.deposit_timestamp;
        let held_secs = if now > deposit_secs { now - deposit_secs } else { 0 };
        let eligible_days = held_secs / SECONDS_PER_DAY;
        let eligible_days = eligible_days.min(config.cycle_days);
        if eligible_days == 0 {
            continue;
        }

        // Calculate user's principal (stablecoin value)
        // principal = user_lp_balance * vault_total_deposited / vault_total_lp_supply
        let vault = query_psp_vault_info(deps.as_ref(), &config)?;
        let principal = if vault.total_lp_supply.is_zero() {
            Uint128::zero()
        } else {
            user_info.lp_balance
                .checked_multiply_ratio(vault.total_deposited, vault.total_lp_supply)
                .map_err(|_| ContractError::ArithmeticOverflow {
                    operation: "user principal calculation".to_string(),
                })?
        };
        if principal.is_zero() {
            continue;
        }

        // yield = principal * investor_apy_bps * eligible_days / (360 * 10_000)
        let numerator = principal
            .checked_mul(Uint128::from(config.investor_apy_bps))
            .map_err(|_| ContractError::ArithmeticOverflow { operation: "principal * apy_bps".to_string() })?
            .checked_mul(Uint128::from(eligible_days))
            .map_err(|_| ContractError::ArithmeticOverflow { operation: "* eligible_days".to_string() })?;
        let denominator = BPS_DENOMINATOR
            .checked_mul(APY_DAYS_BASIS)
            .ok_or_else(|| ContractError::ArithmeticOverflow { operation: "BPS_DENOMINATOR * APY_DAYS_BASIS".to_string() })?;
        let payout = numerator
            .checked_div(Uint128::from(denominator))
            .map_err(|_| ContractError::ArithmeticOverflow { operation: "yield division".to_string() })?;

        if payout.is_zero() {
            continue;
        }

        let current = payouts.get(holder).copied().unwrap_or_else(Uint128::zero);
        let updated = checked_add(current, payout, "current_payout + holder_payout")?;
        payouts.insert(holder.clone(), updated);
        batch_total = checked_add(batch_total, payout, "batch_total + holder_payout")?;
    }

    if !has_more {
        let pre_total = checked_add(active.distributed_amount, batch_total, "distributed_amount + batch_total")?;
        if pre_total > active.cycle_amount {
            return Err(ContractError::PayoutSumMismatch {
                expected: active.cycle_amount,
                actual: pre_total,
            });
        }

        let remainder = active.cycle_amount.checked_sub(pre_total).map_err(|_| ContractError::RemainderAllocationFailed {})?;
        if !remainder.is_zero() {
            if let Some(last_holder) = holders_batch.last() {
                let existing = payouts.get(last_holder).copied().unwrap_or_else(Uint128::zero);
                let updated = checked_add(existing, remainder, "holder_payout + remainder")?;
                payouts.insert(last_holder.clone(), updated);
                batch_total = checked_add(batch_total, remainder, "batch_total + remainder")?;
            } else {
                return Err(ContractError::RemainderAllocationFailed {});
            }
        }
    }

    let mut response = Response::new()
        .add_attribute("action", "disburse_yield_batch")
        .add_attribute("triggered_by", info.sender)
        .add_attribute("cycle_amount", active.cycle_amount)
        .add_attribute("batch_holder_count", holders_batch.len().to_string())
        .add_attribute("batch_total", batch_total)
        .add_attribute("has_more", has_more.to_string());

    for (recipient, amount) in &payouts {
        if amount.is_zero() {
            continue;
        }
        response = response.add_message(WasmMsg::Execute {
            contract_addr: config.yield_reserve.to_string(),
            msg: to_json_binary(&YieldReserveExecuteMsg::Disburse {
                recipient: recipient.clone(),
                amount: *amount,
            })?,
            funds: vec![],
        });
    }

    active.distributed_amount = checked_add(active.distributed_amount, batch_total, "distributed_amount + batch_total")?;

    if has_more {
        let last_holder = holders_batch.last().cloned().ok_or(ContractError::NoIndexedLpHolders {})?;
        active.next_start_after = Some(last_holder.clone());
        state.active_disbursement = Some(active);
        CYCLE_STATE.save(deps.storage, &state)?;

        response = response.add_attribute("finalized", "false").add_attribute("next_start_after", last_holder);
        return Ok(response);
    }

    if active.distributed_amount != active.cycle_amount {
        return Err(ContractError::PayoutSumMismatch {
            expected: active.cycle_amount,
            actual: active.distributed_amount,
        });
    }

    state.completed_cycles = checked_add_u64(state.completed_cycles, 1, "completed_cycles + 1")?;
    state.last_disbursed_at = Some(now);
    state.active_disbursement = None;
    CYCLE_STATE.save(deps.storage, &state)?;

    Ok(response.add_attribute("finalized", "true").add_attribute("completed_cycles", state.completed_cycles.to_string()).add_attribute("distributed_total", active.distributed_amount))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&query_config(deps)?),
        QueryMsg::CycleStatus {} => to_json_binary(&query_cycle_status(deps)?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        admin: config.admin.to_string(),
        psp_pool: config.psp_pool.to_string(),
        yield_reserve: config.yield_reserve.to_string(),
        investor_apy_bps: config.investor_apy_bps,
        cycle_days: config.cycle_days,
        cycle_anchor_time: config.cycle_anchor_time,
    })
}

fn query_cycle_status(deps: Deps) -> StdResult<CycleStatusResponse> {
    let config = CONFIG.load(deps.storage)?;
    let state = CYCLE_STATE.load(deps.storage)?;
    let next_cycle_time = compute_next_cycle_time(&config, state.completed_cycles)
        .map_err(|e| cosmwasm_std::StdError::generic_err(e.to_string()))?;

    let (disbursement_in_progress, next_start_after, distributed_amount, next_cycle_amount) =
        match state.active_disbursement {
            Some(active) => (
                true,
                active.next_start_after,
                active.distributed_amount,
                active.cycle_amount,
            ),
            None => {
                let vault = query_psp_vault_info(deps, &config)
                    .map_err(|e| cosmwasm_std::StdError::generic_err(e.to_string()))?;
                let estimated = if vault.total_lp_supply.is_zero() {
                    Uint128::zero()
                } else {
                    calculate_cycle_amount(
                        vault.total_deposited,
                        config.investor_apy_bps,
                        config.cycle_days,
                    )
                    .map_err(|e| cosmwasm_std::StdError::generic_err(e.to_string()))?
                };
                (false, None, Uint128::zero(), estimated)
            }
        };

    Ok(CycleStatusResponse {
        completed_cycles: state.completed_cycles,
        last_disbursed_at: state.last_disbursed_at,
        next_cycle_time,
        next_cycle_amount,
        disbursement_in_progress,
        next_start_after,
        distributed_amount,
    })
}

fn require_admin(info: &MessageInfo, config: &Config) -> Result<(), ContractError> {
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
    Ok(())
}

fn require_no_funds(info: &MessageInfo) -> Result<(), ContractError> {
    if !info.funds.is_empty() {
        return Err(ContractError::UnexpectedFunds {});
    }
    Ok(())
}

fn normalize_limit(limit: Option<u32>) -> Result<u32, ContractError> {
    let value = limit.unwrap_or(DEFAULT_BATCH_LIMIT);
    if value == 0 {
        return Err(ContractError::InvalidPaginationLimit {});
    }
    Ok(value.min(MAX_BATCH_LIMIT))
}

fn checked_add(a: Uint128, b: Uint128, operation: &str) -> Result<Uint128, ContractError> {
    a.checked_add(b)
        .map_err(|_| ContractError::ArithmeticOverflow {
            operation: operation.to_string(),
        })
}

fn checked_mul(a: Uint128, b: Uint128, operation: &str) -> Result<Uint128, ContractError> {
    a.checked_mul(b)
        .map_err(|_| ContractError::ArithmeticOverflow {
            operation: operation.to_string(),
        })
}

fn checked_div(a: Uint128, b: Uint128, operation: &str) -> Result<Uint128, ContractError> {
    a.checked_div(b)
        .map_err(|_| ContractError::ArithmeticOverflow {
            operation: operation.to_string(),
        })
}

fn checked_add_u32(a: u32, b: u32, operation: &str) -> Result<u32, ContractError> {
    a.checked_add(b)
        .ok_or_else(|| ContractError::ArithmeticOverflow {
            operation: operation.to_string(),
        })
}

fn checked_add_u64(a: u64, b: u64, operation: &str) -> Result<u64, ContractError> {
    a.checked_add(b)
        .ok_or_else(|| ContractError::ArithmeticOverflow {
            operation: operation.to_string(),
        })
}

fn checked_mul_u64(a: u64, b: u64, operation: &str) -> Result<u64, ContractError> {
    a.checked_mul(b)
        .ok_or_else(|| ContractError::ArithmeticOverflow {
            operation: operation.to_string(),
        })
}

fn calculate_cycle_amount(
    total_deposited: Uint128,
    investor_apy_bps: u64,
    cycle_days: u64,
) -> Result<Uint128, ContractError> {
    let numerator = checked_mul(
        checked_mul(
            total_deposited,
            Uint128::from(investor_apy_bps),
            "total_deposited * investor_apy_bps",
        )?,
        Uint128::from(cycle_days),
        "(total_deposited * investor_apy_bps) * cycle_days",
    )?;

    let denominator = BPS_DENOMINATOR
        .checked_mul(APY_DAYS_BASIS)
        .ok_or_else(|| ContractError::ArithmeticOverflow {
            operation: "BPS_DENOMINATOR * APY_DAYS_BASIS".to_string(),
        })?;

    checked_div(
        numerator,
        Uint128::from(denominator),
        "cycle_amount division",
    )
}

fn compute_next_cycle_time(config: &Config, completed_cycles: u64) -> Result<u64, ContractError> {
    let cycle_secs = checked_mul_u64(
        config.cycle_days,
        SECONDS_PER_DAY,
        "cycle_days * SECONDS_PER_DAY",
    )?;
    let next_index = checked_add_u64(completed_cycles, 1, "completed_cycles + 1")?;
    let offset = checked_mul_u64(next_index, cycle_secs, "next_index * cycle_secs")?;

    config
        .cycle_anchor_time
        .checked_add(offset)
        .ok_or_else(|| ContractError::ArithmeticOverflow {
            operation: "cycle_anchor_time + offset".to_string(),
        })
}

fn query_psp_vault_info(deps: Deps, config: &Config) -> Result<PSPPoolVaultInfoResponse, ContractError> {
    deps.querier
        .query_wasm_smart(
            config.psp_pool.to_string(),
            &PSPPoolQueryMsg::VaultInfo {},
        )
        .map_err(ContractError::from)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::new().add_attribute("action", "migrate"))
}

#[cfg(test)]
mod tests {
            use cosmwasm_std::{QuerierResult, WasmQuery, SystemResult, SystemError};
        use cosmwasm_std::{OwnedDeps, Empty};
        use cosmwasm_std::testing::{MockStorage, MockApi, MockQuerier};
    use cosmwasm_std::{ContractResult, from_json};

    fn install_yield_reserve_mock(
        deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier, Empty>,
        yield_reserve: String,
        balance: Uint128,
        denom: String,
    ) {
        let denom_cloned = denom.clone();
        deps.querier.update_wasm(move |query| -> QuerierResult {
            match query {
                WasmQuery::Smart { contract_addr, msg } => {
                    if contract_addr.as_str() == yield_reserve.as_str() {
                        // Only handle Balance query
                        let parsed = from_json::<defa_yield_reserve::msg::QueryMsg>(msg);
                        match parsed {
                                Ok(defa_yield_reserve::msg::QueryMsg::Balance {}) => {
                                    let bin = to_json_binary(&defa_yield_reserve::msg::BalanceResponse {
                                        amount: balance,
                                        denom: denom_cloned.clone(),
                                    }).unwrap();
                                    SystemResult::Ok(ContractResult::Ok(bin))
                                }
                                Ok(_) => SystemResult::Err(SystemError::UnsupportedRequest { kind: "unsupported".to_string() }),
                                Err(err) => {
                                    return SystemResult::Ok(ContractResult::Err(err.to_string()));
                                }
                        }
                    } else {
                        // Fallback: only try PSPPoolQueryMsg, else unsupported
                        let parsed = from_json::<PSPPoolQueryMsg>(msg);
                        let parsed = match parsed {
                            Ok(value) => value,
                            Err(err) => {
                                return SystemResult::Ok(ContractResult::Err(err.to_string()));
                            }
                        };
                        match parsed {
                            PSPPoolQueryMsg::VaultInfo {} => {
                                let bin = to_json_binary(&PSPPoolVaultInfoResponse {
                                    total_deposited: Uint128::zero(),
                                    total_lp_supply: Uint128::zero(),
                                }).unwrap();
                                SystemResult::Ok(ContractResult::Ok(bin))
                            }
                            PSPPoolQueryMsg::LpHolders { .. } => {
                                let bin = to_json_binary(&PSPPoolLpHoldersResponse { holders: vec![] }).unwrap();
                                SystemResult::Ok(ContractResult::Ok(bin))
                            }
                            PSPPoolQueryMsg::UserInfo { .. } => {
                                let bin = to_json_binary(&PSPPoolUserInfoResponse { lp_balance: Uint128::zero(), deposit_timestamp: 0 }).unwrap();
                                SystemResult::Ok(ContractResult::Ok(bin))
                            }
                        }
                    }
                }
                _ => SystemResult::Err(SystemError::UnsupportedRequest { kind: "unsupported".to_string() }),
            }
        });
    }
    use super::*;
    use cosmwasm_std::testing::{
        message_info, mock_dependencies, mock_env,
    };
    use cosmwasm_std::{
        Addr,
    };

    fn addr(label: &str) -> String {
        MockApi::default().addr_make(label).to_string()
    }

    fn instantiate_default(deps: DepsMut, env: &Env) -> (String, String, String) {
            #[cfg(test)]
                // Removed debug print for psp_pool
    let admin = addr("admin");
    let psp_pool = addr("psp_pool");
    let yield_reserve = addr("yield_reserve");

        let msg = InstantiateMsg {
            admin: admin.clone(),
            psp_pool: psp_pool.clone(),
            yield_reserve: yield_reserve.clone(),
            investor_apy_bps: 36_000,
            cycle_days: 7,
            cycle_anchor_time: Some(env.block.time.seconds()),
        };

        let info = message_info(&Addr::unchecked("creator"), &[]);
        instantiate(deps, env.clone(), info, msg).unwrap();

        (admin, psp_pool, yield_reserve)
    }

    fn install_psp_pool_mock(
        deps: &mut OwnedDeps<MockStorage, MockApi, MockQuerier, Empty>,
        psp_pool: String,
        total_deposited: Uint128,
        total_lp_supply: Uint128,
        holders: Vec<String>,
        balances: BTreeMap<String, Uint128>,
    ) {
        deps.querier.update_wasm(move |query| -> QuerierResult {
            match query {
                WasmQuery::Smart { contract_addr, msg } => {
                    if contract_addr.as_str() != psp_pool.as_str() {
                        return SystemResult::Err(SystemError::NoSuchContract { addr: contract_addr.clone() });
                    }

                    let parsed = from_json::<PSPPoolQueryMsg>(msg);
                    let parsed = match parsed {
                        Ok(value) => value,
                        Err(err) => {
                            return SystemResult::Ok(ContractResult::Err(err.to_string()));
                        }
                    };

                    match parsed {
                        PSPPoolQueryMsg::VaultInfo {} => {
                            let bin = to_json_binary(&PSPPoolVaultInfoResponse {
                                total_deposited,
                                total_lp_supply,
                            })
                            .unwrap();
                            SystemResult::Ok(ContractResult::Ok(bin))
                        }
                        PSPPoolQueryMsg::LpHolders { start_after, limit } => {
                            let mut filtered = holders.clone();
                            if let Some(start_after) = start_after {
                                if let Some(idx) =
                                    filtered.iter().position(|h| h == &start_after)
                                {
                                    filtered = filtered.into_iter().skip(idx + 1).collect();
                                }
                            }
                            let take_n = limit.unwrap_or(50) as usize;
                            filtered.truncate(take_n);
                            let bin = to_json_binary(&PSPPoolLpHoldersResponse {
                                holders: filtered,
                            })
                            .unwrap();
                            SystemResult::Ok(ContractResult::Ok(bin))
                        }
                        PSPPoolQueryMsg::UserInfo { address } => {
                            let lp_balance = balances
                                .get(&address)
                                .copied()
                                .unwrap_or_else(Uint128::zero);
                            let deposit_timestamp = 0u64; // Default for test mock
                            let bin = to_json_binary(&PSPPoolUserInfoResponse {
                                lp_balance,
                                deposit_timestamp,
                            })
                            .unwrap();
                            SystemResult::Ok(ContractResult::Ok(bin))
                        }
                    }
                }
                _ => SystemResult::Err(SystemError::UnsupportedRequest {
                    kind: "unsupported".to_string(),
                }),
            }
        });
    }

    #[test]
    fn only_admin_can_update_wiring() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let (admin, psp_pool, yield_reserve) = instantiate_default(deps.as_mut(), &env);
        install_psp_pool_mock(
            &mut deps,
            psp_pool,
            Uint128::new(1_000),
            Uint128::new(1_000),
            vec![addr("lp1")],
            BTreeMap::from([(addr("lp1"), Uint128::new(1_000))]),
        );
        install_yield_reserve_mock(&mut deps, yield_reserve, Uint128::new(1_000_000), "uusd".to_string());

        let not_admin = message_info(&Addr::unchecked(addr("not_admin")), &[]);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            not_admin,
            ExecuteMsg::UpdateWiring {
                psp_pool: Some(addr("new_pool")),
                yield_reserve: None,
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::Unauthorized { .. }));
            if let ContractError::Unauthorized { .. } = err {
                // pass
            } else {
                panic!("Expected Unauthorized error");
            }

        let admin_info = message_info(&Addr::unchecked(admin), &[]);
        execute(
            deps.as_mut(),
            env.clone(),
            admin_info,
            ExecuteMsg::UpdateWiring {
                psp_pool: Some(addr("new_pool")),
                yield_reserve: None,
            },
        )
        .unwrap();
        // Install PSPPool mock for the new address
        install_psp_pool_mock(
            &mut deps,
            addr("new_pool"),
            Uint128::new(1_000),
            Uint128::new(1_000),
            vec![addr("lp1")],
            BTreeMap::from([(addr("lp1"), Uint128::new(1_000))]),
        );
    }

    #[test]
    fn disburse_computes_payouts_on_chain() {
        let mut deps = mock_dependencies();
        let mut env = mock_env();
        let (_, psp_pool, yield_reserve) = instantiate_default(deps.as_mut(), &env);
    install_yield_reserve_mock(&mut deps, yield_reserve.clone(), Uint128::new(1_000_000), "uusd".to_string());

        let lp1 = addr("lp1");
        let lp2 = addr("lp2");
        install_psp_pool_mock(
            &mut deps,
            psp_pool,
            Uint128::new(1_000),
            Uint128::new(1_000),
            vec![lp1.clone(), lp2.clone()],
            BTreeMap::from([
                (lp1.clone(), Uint128::new(400)),
                (lp2.clone(), Uint128::new(600)),
            ]),
        );

        env.block.time = env.block.time.plus_seconds(7 * SECONDS_PER_DAY);

        let res = execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(addr("keeper")), &[]),
            ExecuteMsg::DisburseYield { limit: Some(20) },
        )
        .unwrap();

        assert_eq!(res.messages.len(), 2);
        match &res.messages[0].msg {
            cosmwasm_std::CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr,
                ..
            }) => {
                assert_eq!(contract_addr, &yield_reserve);
            }
            _ => panic!("expected wasm execute"),
        }

        let status: CycleStatusResponse = from_json(
            query(deps.as_ref(), env, QueryMsg::CycleStatus {}).unwrap(),
        )
        .unwrap();
        assert_eq!(status.completed_cycles, 1);
        assert!(!status.disbursement_in_progress);
        assert_eq!(status.next_cycle_amount, Uint128::new(70));
    }

    #[test]
    fn disburse_supports_batches_and_remainder() {
        let mut deps = mock_dependencies();
        let mut env = mock_env();
        let (_, psp_pool, yield_reserve) = instantiate_default(deps.as_mut(), &env);
    install_yield_reserve_mock(&mut deps, yield_reserve, Uint128::new(1_000_000), "uusd".to_string());

        let a = addr("lp_a");
        let b = addr("lp_b");
        let c = addr("lp_c");
        install_psp_pool_mock(
            &mut deps,
            psp_pool,
            Uint128::new(1_429),
            Uint128::new(3),
            vec![a.clone(), b.clone(), c.clone()],
            BTreeMap::from([
                (a.clone(), Uint128::new(1)),
                (b.clone(), Uint128::new(1)),
                (c.clone(), Uint128::new(1)),
            ]),
        );

        env.block.time = env.block.time.plus_seconds(7 * SECONDS_PER_DAY);

        let first = execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(addr("keeper_1")), &[]),
            ExecuteMsg::DisburseYield { limit: Some(2) },
        )
        .unwrap();
        assert_eq!(first.messages.len(), 2);

        let mid_status: CycleStatusResponse = from_json(
            query(deps.as_ref(), env.clone(), QueryMsg::CycleStatus {}).unwrap(),
        )
        .unwrap();
        assert!(mid_status.disbursement_in_progress);
        assert_eq!(mid_status.distributed_amount, Uint128::new(66));

        let second = execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(addr("keeper_2")), &[]),
            ExecuteMsg::DisburseYield { limit: Some(2) },
        )
        .unwrap();
        assert_eq!(second.messages.len(), 1);

        let status: CycleStatusResponse = from_json(
            query(deps.as_ref(), env, QueryMsg::CycleStatus {}).unwrap(),
        )
        .unwrap();
        assert_eq!(status.completed_cycles, 1);
        assert!(!status.disbursement_in_progress);
        assert_eq!(status.next_cycle_amount, Uint128::new(100));
    }

    #[test]
    fn disburse_fails_without_indexed_holders() {
        let mut deps = mock_dependencies();
        let mut env = mock_env();
        let (_, psp_pool, yield_reserve) = instantiate_default(deps.as_mut(), &env);
        install_yield_reserve_mock(&mut deps, yield_reserve, Uint128::new(1_000_000), "uusd".to_string());

        install_psp_pool_mock(
            &mut deps,
            psp_pool,
            Uint128::new(1_000),
            Uint128::new(10),
            vec![],
            BTreeMap::new(),
        );

        env.block.time = env.block.time.plus_seconds(7 * SECONDS_PER_DAY);

        let err = execute(
            deps.as_mut(),
            env,
            message_info(&Addr::unchecked(addr("keeper")), &[]),
            ExecuteMsg::DisburseYield { limit: Some(10) },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::NoIndexedLpHolders {}));
    }
}
