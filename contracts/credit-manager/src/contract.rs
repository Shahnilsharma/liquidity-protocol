use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    entry_point, to_json_binary, Addr, BankMsg, Binary, Coin, Deps, DepsMut, Env,
    MessageInfo, Order, QuerierWrapper, Response, StdError, StdResult, Storage, Uint128,
    WasmMsg, WasmQuery,
};
use cw2::set_contract_version;
use cw_storage_plus::Bound;
use defa_types::{Drawdown, DrawdownStatus, BPS_DENOMINATOR, SECONDS_PER_DAY};

use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, DrawdownResponse, DrawdownsResponse, ExecuteMsg, FeePreviewResponse,
    InstantiateMsg, MigrateMsg, QueryMsg, SystemStatusResponse,
};
use crate::state::{
    Config, DiscretionaryWindow, BLOCKED, CONFIG, CONTRACT_NAME, CONTRACT_VERSION,
    DISCRETIONARY_WINDOW, DRAWDOWNS, NEXT_DRAWDOWN_ID, OUTSTANDING_PRINCIPAL, OVERDUE_COUNT,
};

const MAX_PAGE_LIMIT: u32 = 50;
const DEFAULT_PAGE_LIMIT: u32 = 20;

#[cw_serde]
enum PspPoolExecuteMsg {
    DisburseToPsp { amount: Uint128, recipient: String },
}

#[cw_serde]
enum PspPoolQueryMsg {
    PoolState {},
    OldestQueueWaitDays {},
}

#[cw_serde]
struct PspPoolStateResponse {
    state: String,
    paused: bool,
}

#[cw_serde]
struct OldestQueueWaitDaysResponse {
    days: Option<u64>,
}

#[cw_serde]
enum YieldReserveExecuteMsg {
    ReceiveFrom { amount: Uint128 },
}

/// Instantiates a per-pool CreditManager contract.
///
/// Only a pre-authorized deployer should instantiate this contract via PoolFactory in production.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    if msg.drawdown_limit.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let admin = deps.api.addr_validate(&msg.admin)?;
    let psp_address = deps.api.addr_validate(&msg.psp_address)?;
    let psp_pool = deps.api.addr_validate(&msg.psp_pool)?;
    let yield_reserve = deps.api.addr_validate(&msg.yield_reserve)?;

    let config = Config {
        admin: admin.clone(),
        psp_address: psp_address.clone(),
        psp_pool: psp_pool.clone(),
        yield_reserve: yield_reserve.clone(),
        stablecoin_denom: msg.stablecoin_denom,
        drawdown_limit: msg.drawdown_limit,
        drawdown_tenor_days: msg.drawdown_tenor_days,
        psp_rate_bps_per_day: msg.psp_rate_bps_per_day,
        penalty_rate_bps_per_day: msg.penalty_rate_bps_per_day,
        max_queue_wait_days: msg.max_queue_wait_days,
    };

    CONFIG.save(deps.storage, &config)?;
    NEXT_DRAWDOWN_ID.save(deps.storage, &0u64)?;
    OUTSTANDING_PRINCIPAL.save(deps.storage, &Uint128::zero())?;
    OVERDUE_COUNT.save(deps.storage, &0u64)?;
    BLOCKED.save(deps.storage, &false)?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("admin", admin)
        .add_attribute("psp_address", psp_address)
        .add_attribute("psp_pool", psp_pool)
        .add_attribute("yield_reserve", yield_reserve))
}

/// Executes state-changing CreditManager operations.
///
/// Admin authentication assumes the configured admin address is a 2-of-3 multisig wallet.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::RequestDrawdown { amount } => execute_request_drawdown(deps, env, info, amount),
        ExecuteMsg::Repay { drawdown_id } => execute_repay(deps, env, info, drawdown_id),
        ExecuteMsg::OverrideBlock {
            drawdown_id,
            reason_code,
        } => execute_override_block(deps, env, info, drawdown_id, reason_code),
        ExecuteMsg::SetDiscretionaryWindow {
            start_time,
            end_time,
        } => execute_set_discretionary_window(deps, env, info, start_time, end_time),
        ExecuteMsg::UpdateWiring {
            psp_pool,
            yield_reserve,
            max_queue_wait_days,
        } => execute_update_wiring(deps, info, psp_pool, yield_reserve, max_queue_wait_days),
        ExecuteMsg::UpdatePspAddress { new_psp_address } => {
            execute_update_psp_address(deps, info, new_psp_address)
        }
        ExecuteMsg::SettleUnutilizedFee {} => execute_settle_unutilized_fee(),
    }
}

fn execute_request_drawdown(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    require_psp(&info.sender, &config.psp_address)?;

    if amount.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let pool_state = query_pool_state(&deps.querier, &config.psp_pool)?;
    if pool_state.paused || pool_state.state != "Active" {
        return Err(ContractError::PoolNotActive {
            current_state: pool_state.state,
        });
    }

    let max_queue_wait_days = config
        .max_queue_wait_days
        .ok_or(ContractError::MaxQueueWaitUnset {})?;

    if let Some(wait_days) = query_oldest_queue_wait_days(&deps.querier, &config.psp_pool)? {
        if wait_days > max_queue_wait_days {
            BLOCKED.save(deps.storage, &true)?;
            return Err(ContractError::QueueWaitBreach {
                wait_days,
                max_queue_wait_days,
            });
        }
    }

    let (_, blocked) = recompute_overdue_and_block(
        deps.storage,
        env.block.time.seconds(),
        &config,
    )?;
    if blocked {
        return Err(ContractError::OverdueDrawdownBlocks {});
    }

    let outstanding = OUTSTANDING_PRINCIPAL.load(deps.storage)?;
    let new_outstanding = checked_add(outstanding, amount, "outstanding + request")?;
    if new_outstanding > config.drawdown_limit {
        return Err(ContractError::DrawdownLimitExceeded {
            requested: amount,
            outstanding,
            limit: config.drawdown_limit,
        });
    }

    let pool_balance = query_bank_balance(&deps.querier, &config.psp_pool, &config.stablecoin_denom)?;
    let minimum_reserve = if pool_balance > config.drawdown_limit {
        checked_sub(pool_balance, config.drawdown_limit, "pool_balance - drawdown_limit")?
    } else {
        Uint128::zero()
    };
    let post_drawdown_balance = checked_sub(pool_balance, amount, "pool_balance - drawdown")?;
    if post_drawdown_balance < minimum_reserve {
        return Err(ContractError::InsufficientPoolReserve {
            post_drawdown_balance,
            minimum_reserve,
        });
    }

    let drawdown_id = NEXT_DRAWDOWN_ID.load(deps.storage)?;
    let drawdown_tenor_secs = config
        .drawdown_tenor_days
        .checked_mul(SECONDS_PER_DAY)
        .ok_or_else(|| ContractError::ArithmeticOverflow {
            operation: "drawdown_tenor_days * SECONDS_PER_DAY".to_string(),
        })?;
    let due_date = env
        .block
        .time
        .seconds()
        .checked_add(drawdown_tenor_secs)
        .ok_or_else(|| ContractError::ArithmeticOverflow {
            operation: "drawdown_timestamp + drawdown_tenor_secs".to_string(),
        })?;

    let drawdown = Drawdown {
        id: drawdown_id,
        principal: amount,
        timestamp: env.block.time.seconds(),
        due_date,
        status: DrawdownStatus::Active,
        settled_at: None,
        fee_paid: None,
    };

    DRAWDOWNS.save(deps.storage, drawdown_id, &drawdown)?;
    let next_drawdown_id = checked_add_u64(drawdown_id, 1, "drawdown_id + 1")?;
    NEXT_DRAWDOWN_ID.save(deps.storage, &next_drawdown_id)?;
    OUTSTANDING_PRINCIPAL.save(deps.storage, &new_outstanding)?;

    let disburse_msg = WasmMsg::Execute {
        contract_addr: config.psp_pool.to_string(),
        msg: to_json_binary(&PspPoolExecuteMsg::DisburseToPsp {
            amount,
            recipient: config.psp_address.to_string(),
        })?,
        funds: vec![],
    };

    Ok(Response::new()
        .add_message(disburse_msg)
        .add_attribute("action", "request_drawdown")
        .add_attribute("drawdown_id", drawdown_id.to_string())
        .add_attribute("psp_address", config.psp_address)
        .add_attribute("amount", amount)
        .add_attribute("outstanding_principal", new_outstanding))
}

fn execute_repay(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    drawdown_id: u64,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    require_psp(&info.sender, &config.psp_address)?;

    let mut drawdown = DRAWDOWNS
        .may_load(deps.storage, drawdown_id)?
        .ok_or(ContractError::DrawdownNotFound { drawdown_id })?;

    if !matches!(drawdown.status, DrawdownStatus::Active | DrawdownStatus::Overdue) {
        return Err(ContractError::DrawdownNotRepayable {
            drawdown_id,
            status: format!("{:?}", drawdown.status),
        });
    }

    let drawdown_tenor_secs = config
        .drawdown_tenor_days
        .checked_mul(SECONDS_PER_DAY)
        .ok_or_else(|| ContractError::ArithmeticOverflow {
            operation: "drawdown_tenor_days * SECONDS_PER_DAY".to_string(),
        })?;

    let fee = calculate_fee(
        drawdown.principal,
        drawdown.timestamp,
        env.block.time.seconds(),
        drawdown_tenor_secs,
        config.psp_rate_bps_per_day,
        config.penalty_rate_bps_per_day,
    )?;
    let total_repay = checked_add(drawdown.principal, fee, "principal + fee")?;

    let received = extract_single_exact_fund(&info, &config.stablecoin_denom, total_repay)?;

    drawdown.status = DrawdownStatus::Settled;
    drawdown.settled_at = Some(env.block.time.seconds());
    drawdown.fee_paid = Some(fee);
    DRAWDOWNS.save(deps.storage, drawdown_id, &drawdown)?;

    let outstanding = OUTSTANDING_PRINCIPAL.load(deps.storage)?;
    let new_outstanding = checked_sub(outstanding, drawdown.principal, "outstanding - principal")?;
    OUTSTANDING_PRINCIPAL.save(deps.storage, &new_outstanding)?;

    let (overdue_count, blocked) = recompute_overdue_and_block(
        deps.storage,
        env.block.time.seconds(),
        &config,
    )?;

    let principal_msg = BankMsg::Send {
        to_address: config.psp_pool.to_string(),
        amount: vec![Coin {
            denom: config.stablecoin_denom.clone(),
            amount: drawdown.principal,
        }],
    };

    let mut response = Response::new()
        .add_message(principal_msg)
        .add_attribute("action", "repay")
        .add_attribute("drawdown_id", drawdown_id.to_string())
        .add_attribute("principal", drawdown.principal)
        .add_attribute("fee", fee)
        .add_attribute("received", received)
        .add_attribute("blocked", blocked.to_string())
        .add_attribute("overdue_count", overdue_count.to_string());

    if !fee.is_zero() {
        let reserve_msg = WasmMsg::Execute {
            contract_addr: config.yield_reserve.to_string(),
            msg: to_json_binary(&YieldReserveExecuteMsg::ReceiveFrom { amount: fee })?,
            funds: vec![Coin {
                denom: config.stablecoin_denom,
                amount: fee,
            }],
        };
        response = response.add_message(reserve_msg);
    }

    Ok(response)
}

fn execute_override_block(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    drawdown_id: u64,
    reason_code: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    require_admin(&info.sender, &config.admin)?;

    let _ = DRAWDOWNS
        .may_load(deps.storage, drawdown_id)?
        .ok_or(ContractError::DrawdownNotFound { drawdown_id })?;

    if !discretionary_window_is_active(deps.storage, env.block.time.seconds())? {
        return Err(ContractError::InvalidDiscretionaryWindow {});
    }

    BLOCKED.save(deps.storage, &false)?;

    Ok(Response::new()
        .add_attribute("action", "override_block")
        .add_attribute("timestamp", env.block.time.seconds().to_string())
        .add_attribute("admin_addr", info.sender)
        .add_attribute("drawdown_id", drawdown_id.to_string())
        .add_attribute("reason_code", reason_code)
        .add_attribute("blocked", "false"))
}

fn execute_set_discretionary_window(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    start_time: u64,
    end_time: u64,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    require_admin(&info.sender, &config.admin)?;

    if end_time <= start_time || end_time <= env.block.time.seconds() {
        return Err(ContractError::InvalidDiscretionaryWindow {});
    }

    DISCRETIONARY_WINDOW.save(
        deps.storage,
        &DiscretionaryWindow {
            start_time,
            end_time,
        },
    )?;

    Ok(Response::new()
        .add_attribute("action", "set_discretionary_window")
        .add_attribute("admin_addr", info.sender)
        .add_attribute("start_time", start_time.to_string())
        .add_attribute("end_time", end_time.to_string()))
}

fn execute_update_psp_address(
    deps: DepsMut,
    info: MessageInfo,
    new_psp_address: String,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    require_admin(&info.sender, &config.admin)?;

    for item in DRAWDOWNS.range(deps.storage, None, None, Order::Ascending) {
        let (_, drawdown) = item?;
        if matches!(drawdown.status, DrawdownStatus::Active) {
            return Err(ContractError::ActiveDrawdownsExist {});
        }
    }

    let validated = deps.api.addr_validate(&new_psp_address)?;
    config.psp_address = validated.clone();
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("action", "update_psp_address")
        .add_attribute("admin_addr", info.sender)
        .add_attribute("new_psp_address", validated))
}

fn execute_update_wiring(
    deps: DepsMut,
    info: MessageInfo,
    psp_pool: Option<String>,
    yield_reserve: Option<String>,
    max_queue_wait_days: Option<u64>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    require_admin(&info.sender, &config.admin)?;

    let mut response = Response::new()
        .add_attribute("action", "update_wiring")
        .add_attribute("admin_addr", info.sender.to_string());

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

    if let Some(days) = max_queue_wait_days {
        config.max_queue_wait_days = Some(days);
        response = response.add_attribute("max_queue_wait_days", days.to_string());
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(response)
}

fn execute_settle_unutilized_fee() -> Result<Response, ContractError> {
    Err(ContractError::UnutilizedFeeFormulaUnset {
        message: "Formula pending DeFa team confirmation. See architecture doc open item #1.".to_string(),
    })
}

/// Queries contract read-only state.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&query_config(deps)?),
        QueryMsg::Drawdown { drawdown_id } => to_json_binary(&query_drawdown(deps, drawdown_id)?),
        QueryMsg::Drawdowns { start_after, limit } => {
            to_json_binary(&query_drawdowns(deps, start_after, limit)?)
        }
        QueryMsg::SystemStatus {} => to_json_binary(&query_system_status(deps)?),
        QueryMsg::FeePreview {
            principal,
            drawdown_timestamp,
            current_timestamp,
        } => to_json_binary(&query_fee_preview(
            deps,
            principal,
            drawdown_timestamp,
            current_timestamp,
        )?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        admin: config.admin.to_string(),
        psp_address: config.psp_address.to_string(),
        psp_pool: config.psp_pool.to_string(),
        yield_reserve: config.yield_reserve.to_string(),
        stablecoin_denom: config.stablecoin_denom,
        drawdown_limit: config.drawdown_limit,
        drawdown_tenor_days: config.drawdown_tenor_days,
        psp_rate_bps_per_day: config.psp_rate_bps_per_day,
        penalty_rate_bps_per_day: config.penalty_rate_bps_per_day,
        max_queue_wait_days: config.max_queue_wait_days,
    })
}

fn query_drawdown(deps: Deps, drawdown_id: u64) -> StdResult<DrawdownResponse> {
    let drawdown = DRAWDOWNS
        .may_load(deps.storage, drawdown_id)?
        .ok_or_else(|| StdError::generic_err(format!("Drawdown {} not found", drawdown_id)))?;

    Ok(DrawdownResponse { drawdown })
}

fn query_drawdowns(
    deps: Deps,
    start_after: Option<u64>,
    limit: Option<u32>,
) -> StdResult<DrawdownsResponse> {
    let page_size = limit.unwrap_or(DEFAULT_PAGE_LIMIT).min(MAX_PAGE_LIMIT) as usize;
    let start = start_after.map(Bound::exclusive);

    let drawdowns: StdResult<Vec<Drawdown>> = DRAWDOWNS
        .range(deps.storage, start, None, Order::Ascending)
        .take(page_size)
        .map(|item| item.map(|(_, drawdown)| drawdown))
        .collect();

    Ok(DrawdownsResponse {
        drawdowns: drawdowns?,
    })
}

fn query_system_status(deps: Deps) -> StdResult<SystemStatusResponse> {
    let blocked = BLOCKED.may_load(deps.storage)?.unwrap_or(false);
    let outstanding_principal = OUTSTANDING_PRINCIPAL
        .may_load(deps.storage)?
        .unwrap_or_default();
    let overdue_count = OVERDUE_COUNT.may_load(deps.storage)?.unwrap_or(0);
    let next_drawdown_id = NEXT_DRAWDOWN_ID.may_load(deps.storage)?.unwrap_or(0);
    let discretionary_window_end = DISCRETIONARY_WINDOW
        .may_load(deps.storage)?
        .map(|window| window.end_time);

    Ok(SystemStatusResponse {
        blocked,
        outstanding_principal,
        overdue_count,
        next_drawdown_id,
        discretionary_window_end,
    })
}

fn query_fee_preview(
    deps: Deps,
    principal: Uint128,
    drawdown_timestamp: u64,
    current_timestamp: u64,
) -> StdResult<FeePreviewResponse> {
    let config = CONFIG.load(deps.storage)?;
    let drawdown_tenor_secs = config
        .drawdown_tenor_days
        .checked_mul(SECONDS_PER_DAY)
        .ok_or_else(|| StdError::generic_err("drawdown tenor overflow"))?;

    let fee = calculate_fee(
        principal,
        drawdown_timestamp,
        current_timestamp,
        drawdown_tenor_secs,
        config.psp_rate_bps_per_day,
        config.penalty_rate_bps_per_day,
    )
    .map_err(|e| StdError::generic_err(e.to_string()))?;

    Ok(FeePreviewResponse { fee })
}

/// Calculates the repayment fee lazily at repayment time.
///
/// Penalty rate replaces the PSP rate for all seconds beyond tenor+24h.
pub fn calculate_fee(
    principal: Uint128,
    drawdown_timestamp: u64,
    current_timestamp: u64,
    drawdown_tenor_secs: u64,
    psp_rate_bps_per_day: u64,
    penalty_rate_bps_per_day: u64,
) -> Result<Uint128, ContractError> {
    let elapsed_secs = current_timestamp
        .checked_sub(drawdown_timestamp)
        .ok_or_else(|| ContractError::InvalidTimestamp {
            detail: format!(
                "current_timestamp ({}) < drawdown_timestamp ({})",
                current_timestamp, drawdown_timestamp
            ),
        })?;
    // Normal period = tenor + 24h grace
    let normal_period_end_secs = drawdown_tenor_secs + SECONDS_PER_DAY;

    // Calculate integer days for normal and penalty periods
    let (normal_days, penalty_days) = if elapsed_secs <= normal_period_end_secs {
        (elapsed_secs / SECONDS_PER_DAY, 0u64)
    } else {
        (
            normal_period_end_secs / SECONDS_PER_DAY,
            // Ceiling division for penalty days: any partial penalty day counts as a full day
            ((elapsed_secs - normal_period_end_secs) + SECONDS_PER_DAY - 1) / SECONDS_PER_DAY,
        )
    };

    let normal_fee = checked_div(
        checked_mul(
            checked_mul(
                principal,
                Uint128::from(psp_rate_bps_per_day),
                "principal * psp_rate_bps_per_day",
            )?,
            Uint128::from(normal_days),
            "normal_fee numerator * normal_days",
        )?,
        Uint128::from(BPS_DENOMINATOR),
        "normal_fee denominator division",
    )?;

    let penalty_fee = checked_div(
        checked_mul(
            checked_mul(
                principal,
                Uint128::from(penalty_rate_bps_per_day),
                "principal * penalty_rate_bps_per_day",
            )?,
            Uint128::from(penalty_days),
            "penalty_fee numerator * penalty_days",
        )?,
        Uint128::from(BPS_DENOMINATOR),
        "penalty_fee denominator division",
    )?;

    // Debug print for test diagnosis
    #[cfg(test)]
    println!("calculate_fee debug: normal_days={}, penalty_days={}, normal_fee={}, penalty_fee={}", normal_days, penalty_days, normal_fee, penalty_fee);

    checked_add(normal_fee, penalty_fee, "normal_fee + penalty_fee")
}

fn require_admin(sender: &Addr, admin: &Addr) -> Result<(), ContractError> {
    if sender != admin {
        return Err(ContractError::Unauthorized {
            caller: sender.to_string(),
            expected: admin.to_string(),
        });
    }
    Ok(())
}

fn require_psp(sender: &Addr, psp_address: &Addr) -> Result<(), ContractError> {
    if sender != psp_address {
        return Err(ContractError::PspOnly {});
    }
    Ok(())
}

fn extract_single_exact_fund(
    info: &MessageInfo,
    denom: &str,
    expected_amount: Uint128,
) -> Result<Uint128, ContractError> {
    if info.funds.len() != 1 || info.funds[0].denom != denom {
        return Err(ContractError::InvalidFunds {
            denom: denom.to_string(),
        });
    }

    let received = info.funds[0].amount;
    if received != expected_amount {
        return Err(ContractError::RepayFundsMismatch {
            expected: expected_amount,
            received,
            denom: denom.to_string(),
        });
    }

    Ok(received)
}

fn recompute_overdue_and_block(
    storage: &mut dyn Storage,
    current_time: u64,
    config: &Config,
) -> Result<(u64, bool), ContractError> {
    let overdue_threshold_extra = SECONDS_PER_DAY
        .checked_mul(2)
        .ok_or_else(|| ContractError::ArithmeticOverflow {
            operation: "SECONDS_PER_DAY * 2".to_string(),
        })?;

    let mut to_mark_overdue: Vec<u64> = Vec::new();
    let mut overdue_count = 0u64;

    for item in DRAWDOWNS.range(storage, None, None, Order::Ascending) {
        let (drawdown_id, drawdown) = item?;

        match drawdown.status {
            DrawdownStatus::Overdue => {
                overdue_count = checked_add_u64(overdue_count, 1, "overdue_count + 1")?;
            }
            DrawdownStatus::Active => {
                let overdue_time = drawdown
                    .due_date
                    .checked_add(overdue_threshold_extra)
                    .ok_or_else(|| ContractError::ArithmeticOverflow {
                        operation: "drawdown.due_date + overdue_threshold_extra".to_string(),
                    })?;
                if current_time > overdue_time {
                    to_mark_overdue.push(drawdown_id);
                    overdue_count = checked_add_u64(overdue_count, 1, "overdue_count + 1")?;
                }
            }
            DrawdownStatus::Settled => {}
        }
    }

    for drawdown_id in to_mark_overdue {
        DRAWDOWNS.update(storage, drawdown_id, |maybe| -> Result<_, ContractError> {
            let mut drawdown = maybe.ok_or(ContractError::DrawdownNotFound { drawdown_id })?;
            drawdown.status = DrawdownStatus::Overdue;
            Ok(drawdown)
        })?;
    }

    OVERDUE_COUNT.save(storage, &overdue_count)?;

    if overdue_count == 0 {
        BLOCKED.save(storage, &false)?;
        return Ok((0, false));
    }

    if !discretionary_window_is_active(storage, current_time)? {
        BLOCKED.save(storage, &true)?;
    }

    let blocked = BLOCKED.may_load(storage)?.unwrap_or(true);
    let _ = config;
    Ok((overdue_count, blocked))
}

fn discretionary_window_is_active(
    storage: &dyn Storage,
    current_time: u64,
) -> Result<bool, ContractError> {
    let maybe_window = DISCRETIONARY_WINDOW.may_load(storage)?;
    Ok(match maybe_window {
        Some(window) => current_time >= window.start_time && current_time <= window.end_time,
        None => false,
    })
}

fn query_pool_state(
    querier: &QuerierWrapper,
    psp_pool: &Addr,
) -> Result<PspPoolStateResponse, ContractError> {
    querier
        .query(&WasmQuery::Smart {
            contract_addr: psp_pool.to_string(),
            msg: to_json_binary(&PspPoolQueryMsg::PoolState {})?,
        }
        .into())
        .map_err(ContractError::from)
}

fn query_oldest_queue_wait_days(
    querier: &QuerierWrapper,
    psp_pool: &Addr,
) -> Result<Option<u64>, ContractError> {
    let response: OldestQueueWaitDaysResponse = querier
        .query(&WasmQuery::Smart {
            contract_addr: psp_pool.to_string(),
            msg: to_json_binary(&PspPoolQueryMsg::OldestQueueWaitDays {})?,
        }
        .into())
        .map_err(|_| ContractError::QueueWaitQueryFailed {})?;

    Ok(response.days)
}

fn query_bank_balance(
    querier: &QuerierWrapper,
    address: &Addr,
    denom: &str,
) -> Result<Uint128, ContractError> {
    let balance = querier.query_balance(address, denom)?;
    Ok(balance.amount)
}

fn checked_add(a: Uint128, b: Uint128, operation: &str) -> Result<Uint128, ContractError> {
    a.checked_add(b)
        .map_err(|_| ContractError::ArithmeticOverflow {
            operation: operation.to_string(),
        })
}

fn checked_sub(a: Uint128, b: Uint128, operation: &str) -> Result<Uint128, ContractError> {
    a.checked_sub(b)
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

fn checked_add_u64(a: u64, b: u64, operation: &str) -> Result<u64, ContractError> {
    a.checked_add(b)
        .ok_or_else(|| ContractError::ArithmeticOverflow {
            operation: operation.to_string(),
        })
}

/// Runs state migration for future versions.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::new().add_attribute("action", "migrate"))
}

#[cfg(test)]
mod tests {
    use super::calculate_fee;
    use cosmwasm_std::Uint128;
    use defa_types::SECONDS_PER_DAY;

    #[test]
    fn fee_uses_psp_rate_until_tenor() {
        let principal = Uint128::new(1_000_000);
        let tenor_secs = 2 * SECONDS_PER_DAY;
        let elapsed = tenor_secs;

        let fee = calculate_fee(principal, 0, elapsed, tenor_secs, 10, 25).unwrap();
        // 2 days at 10 bps/day on 1,000,000 = 2,000
        assert_eq!(fee, Uint128::new(2_000));
    }

    #[test]
    fn fee_replaces_with_penalty_after_tenor() {
        let principal = Uint128::new(1_000_000);
        let tenor_secs = 2 * SECONDS_PER_DAY;
        // At elapsed = 3 days, normal period (tenor + 24h grace) just ends, so all 3 days are normal
        let elapsed = tenor_secs + SECONDS_PER_DAY;

        let fee = calculate_fee(principal, 0, elapsed, tenor_secs, 10, 25).unwrap();
        // Normal: 3 days at 10 bps = 3,000, Penalty: 0 days
        assert_eq!(fee, Uint128::new(3_000));
    }

    #[test]
    fn fee_calculation_all_boundaries() {
        // Law: fee = principal × rate_bps × elapsed_days / 10_000, using normal period = tenor + 24h grace (3 days for tenor=2d)
        let principal = Uint128::new(1_000_000);
        let tenor_secs = 2 * SECONDS_PER_DAY;
        let psp_rate = 10u64; // 10 bps/day
        let penalty_rate = 25u64; // 25 bps/day

        // 1. elapsed = 2 days (2 normal, 0 penalty)
        let elapsed = 2 * SECONDS_PER_DAY;
        let fee = calculate_fee(principal, 0, elapsed, tenor_secs, psp_rate, penalty_rate).unwrap();
        assert_eq!(fee, Uint128::new(2_000));

        // 2. elapsed = 3 days (3 normal, 0 penalty)
        let elapsed = 3 * SECONDS_PER_DAY;
        let fee2 = calculate_fee(principal, 0, elapsed, tenor_secs, psp_rate, penalty_rate).unwrap();
        assert_eq!(fee2, Uint128::new(3_000));

        // 3. elapsed = 4 days (3 normal, 1 penalty)
        let elapsed = 4 * SECONDS_PER_DAY;
        let fee3 = calculate_fee(principal, 0, elapsed, tenor_secs, psp_rate, penalty_rate).unwrap();
        assert_eq!(fee3, Uint128::new(5_500));

        // 4. elapsed = 5 days (3 normal, 2 penalty)
        let elapsed = 5 * SECONDS_PER_DAY;
        let fee4 = calculate_fee(principal, 0, elapsed, tenor_secs, psp_rate, penalty_rate).unwrap();
        assert_eq!(fee4, Uint128::new(8_000));
    }
}
