# Code dump for '/home/shahnil/Desktop_extracted/Desktop/NewFolder/lp-transfer/liquidity-protocol/contracts' (pattern: *.rs)

## credit-manager/src/contract.rs

```
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

```

## credit-manager/src/error.rs

```
use cosmwasm_std::{StdError, Uint128};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized: caller={caller}, expected={expected}")]
    Unauthorized { caller: String, expected: String },

    #[error("Only the configured PSP address can call this endpoint")]
    PspOnly {},

    #[error("Invalid zero amount")]
    InvalidZeroAmount {},

    #[error("Arithmetic overflow while computing {operation}")]
    ArithmeticOverflow { operation: String },

    #[error("Contract is blocked due to overdue drawdowns")]
    ContractBlocked {},

    #[error("Drawdown {drawdown_id} not found")]
    DrawdownNotFound { drawdown_id: u64 },

    #[error("Drawdown {drawdown_id} is not repayable in status {status}")]
    DrawdownNotRepayable { drawdown_id: u64, status: String },

    #[error("Drawdown limit exceeded: requested={requested}, outstanding={outstanding}, limit={limit}")]
    DrawdownLimitExceeded {
        requested: Uint128,
        outstanding: Uint128,
        limit: Uint128,
    },

    #[error("Pool state must be Active but is {current_state}")]
    PoolNotActive { current_state: String },

    #[error("A drawdown exceeded tenor+48h and blocks new drawdowns")]
    OverdueDrawdownBlocks {},

    #[error("max_queue_wait_days is unset. OPEN ITEM 2 must be resolved before queue checks")]
    MaxQueueWaitUnset {},

    #[error("Could not query queue wait age from PSPPool")]
    QueueWaitQueryFailed {},

    #[error("Queue wait breach: wait_days={wait_days}, max_queue_wait_days={max_queue_wait_days}")]
    QueueWaitBreach {
        wait_days: u64,
        max_queue_wait_days: u64,
    },

    #[error("Pool reserve check failed: post_drawdown_balance={post_drawdown_balance}, minimum_reserve={minimum_reserve}")]
    InsufficientPoolReserve {
        post_drawdown_balance: Uint128,
        minimum_reserve: Uint128,
    },

    #[error("Repay funds mismatch: expected {expected} {denom}, received {received} {denom}")]
    RepayFundsMismatch {
        expected: Uint128,
        received: Uint128,
        denom: String,
    },

    #[error("Expected a single {denom} coin in funds")]
    InvalidFunds { denom: String },

    #[error("Discretionary window is invalid or inactive")]
    InvalidDiscretionaryWindow {},

    #[error("Active drawdowns exist; PSP address update is blocked")]
    ActiveDrawdownsExist {},

    #[error("Unutilized fee formula is not configured yet: {message}")]
    UnutilizedFeeFormulaUnset { message: String },

    #[error("Invalid timestamp: {detail}")]
    InvalidTimestamp { detail: String },
}

```

## credit-manager/src/lib.rs

```
pub mod contract;
mod error;
pub mod msg;
pub mod state;

pub use crate::error::ContractError;

```

## credit-manager/src/msg.rs

```
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;
use defa_types::Drawdown;

#[cw_serde]
pub struct InstantiateMsg {
    pub admin: String,
    pub psp_address: String,
    pub psp_pool: String,
    pub yield_reserve: String,
    pub stablecoin_denom: String,
    pub drawdown_limit: Uint128,
    pub drawdown_tenor_days: u64,
    pub psp_rate_bps_per_day: u64,
    pub penalty_rate_bps_per_day: u64,
    pub max_queue_wait_days: Option<u64>,
}

#[cw_serde]
pub enum ExecuteMsg {
    RequestDrawdown { amount: Uint128 },
    Repay { drawdown_id: u64 },
    OverrideBlock { drawdown_id: u64, reason_code: String },
    SetDiscretionaryWindow { start_time: u64, end_time: u64 },
    UpdateWiring {
        psp_pool: Option<String>,
        yield_reserve: Option<String>,
        max_queue_wait_days: Option<u64>,
    },
    UpdatePspAddress { new_psp_address: String },
    SettleUnutilizedFee {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},
    #[returns(DrawdownResponse)]
    Drawdown { drawdown_id: u64 },
    #[returns(DrawdownsResponse)]
    Drawdowns {
        start_after: Option<u64>,
        limit: Option<u32>,
    },
    #[returns(SystemStatusResponse)]
    SystemStatus {},
    #[returns(FeePreviewResponse)]
    FeePreview {
        principal: Uint128,
        drawdown_timestamp: u64,
        current_timestamp: u64,
    },
}

#[cw_serde]
pub struct ConfigResponse {
    pub admin: String,
    pub psp_address: String,
    pub psp_pool: String,
    pub yield_reserve: String,
    pub stablecoin_denom: String,
    pub drawdown_limit: Uint128,
    pub drawdown_tenor_days: u64,
    pub psp_rate_bps_per_day: u64,
    pub penalty_rate_bps_per_day: u64,
    pub max_queue_wait_days: Option<u64>,
}

#[cw_serde]
pub struct DrawdownResponse {
    pub drawdown: Drawdown,
}

#[cw_serde]
pub struct DrawdownsResponse {
    pub drawdowns: Vec<Drawdown>,
}

#[cw_serde]
pub struct SystemStatusResponse {
    pub blocked: bool,
    pub outstanding_principal: Uint128,
    pub overdue_count: u64,
    pub next_drawdown_id: u64,
    pub discretionary_window_end: Option<u64>,
}

#[cw_serde]
pub struct FeePreviewResponse {
    pub fee: Uint128,
}

#[cw_serde]
pub struct MigrateMsg {}

```

## credit-manager/src/state.rs

```
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};
use defa_types::Drawdown;

#[cw_serde]
pub struct Config {
    pub admin: Addr,
    pub psp_address: Addr,
    pub psp_pool: Addr,
    pub yield_reserve: Addr,
    pub stablecoin_denom: String,
    pub drawdown_limit: Uint128,
    pub drawdown_tenor_days: u64,
    pub psp_rate_bps_per_day: u64,
    pub penalty_rate_bps_per_day: u64,
    pub max_queue_wait_days: Option<u64>,
}

#[cw_serde]
pub struct DiscretionaryWindow {
    pub start_time: u64,
    pub end_time: u64,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const DRAWDOWNS: Map<u64, Drawdown> = Map::new("drawdowns");
pub const NEXT_DRAWDOWN_ID: Item<u64> = Item::new("next_drawdown_id");
pub const OUTSTANDING_PRINCIPAL: Item<Uint128> = Item::new("outstanding_principal");
pub const OVERDUE_COUNT: Item<u64> = Item::new("overdue_count");
pub const BLOCKED: Item<bool> = Item::new("blocked");
pub const DISCRETIONARY_WINDOW: Item<DiscretionaryWindow> = Item::new("discretionary_window");

pub const CONTRACT_NAME: &str = "crates.io:defa-credit-manager";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

```

## pool-factory/src/contract.rs

```
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
    psp_pool_init_msg: Binary,
    credit_manager_init_msg: Binary,
    yield_distributor_init_msg: Binary,
    yield_reserve_init_msg: Binary,
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

    PENDING_POOL_CREATION.save(
        deps.storage,
        &PendingPoolCreation {
            pool_id,
            label_prefix: label_prefix.clone(),
            created_at: env.block.time.seconds(),
            psp_pool_init_msg: psp_pool_init_msg.clone(),
            credit_manager_init_msg,
            yield_distributor_init_msg,
            yield_reserve_init_msg,
            psp_pool: None,
            credit_manager: None,
            yield_distributor: None,
        },
    )?;

    let instantiate_psp_pool = WasmMsg::Instantiate {
        admin: Some(config.admin.to_string()),
        code_id: config.psp_pool_code_id,
        msg: psp_pool_init_msg,
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

```

## pool-factory/src/error.rs

```
use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized: caller={caller}, expected={expected}")]
    Unauthorized { caller: String, expected: String },

    #[error("A pool creation is already in progress")]
    PendingCreationInProgress {},

    #[error("No pending pool creation found")]
    PendingCreationNotFound {},

    #[error("Invalid reply id: {reply_id}")]
    InvalidReplyId { reply_id: u64 },

    #[error("Missing instantiate address in reply")]
    MissingReplyAddress {},

    #[error("Submessage failed: {reason}")]
    SubMsgFailed { reason: String },

    #[error("Pool creation state is incomplete")]
    IncompletePendingCreation {},

    #[error("Pagination limit must be greater than zero")]
    InvalidPaginationLimit {},

    #[error("Label prefix cannot be empty")]
    InvalidLabelPrefix {},

    #[error("Arithmetic overflow while computing {operation}")]
    ArithmeticOverflow { operation: String },
}

```

## pool-factory/src/lib.rs

```
pub mod contract;
mod error;
pub mod msg;
pub mod state;

pub use crate::error::ContractError;

```

## pool-factory/src/msg.rs

```
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Binary;

#[cw_serde]
pub struct InstantiateMsg {
    pub admin: String,
    pub psp_pool_code_id: u64,
    pub credit_manager_code_id: u64,
    pub yield_distributor_code_id: u64,
    pub yield_reserve_code_id: u64,
}

#[cw_serde]
pub enum ExecuteMsg {
    SetCodeIds {
        psp_pool_code_id: u64,
        credit_manager_code_id: u64,
        yield_distributor_code_id: u64,
        yield_reserve_code_id: u64,
    },
    CreatePool {
        label_prefix: String,
        psp_pool_init_msg: Binary,
        credit_manager_init_msg: Binary,
        yield_distributor_init_msg: Binary,
        yield_reserve_init_msg: Binary,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},
    #[returns(PoolResponse)]
    Pool { pool_id: u64 },
    #[returns(PoolsResponse)]
    Pools { start_after: Option<u64>, limit: Option<u32> },
}

#[cw_serde]
pub struct ConfigResponse {
    pub admin: String,
    pub psp_pool_code_id: u64,
    pub credit_manager_code_id: u64,
    pub yield_distributor_code_id: u64,
    pub yield_reserve_code_id: u64,
}

#[cw_serde]
pub struct PoolRecordResponse {
    pub pool_id: u64,
    pub psp_pool: String,
    pub credit_manager: String,
    pub yield_distributor: String,
    pub yield_reserve: String,
    pub created_at: u64,
}

#[cw_serde]
pub struct PoolResponse {
    pub pool: Option<PoolRecordResponse>,
}

#[cw_serde]
pub struct PoolsResponse {
    pub pools: Vec<PoolRecordResponse>,
}

#[cw_serde]
pub struct MigrateMsg {}

```

## pool-factory/src/state.rs

```
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Binary};
use cw_storage_plus::{Item, Map};

#[cw_serde]
pub struct Config {
    pub admin: Addr,
    pub psp_pool_code_id: u64,
    pub credit_manager_code_id: u64,
    pub yield_distributor_code_id: u64,
    pub yield_reserve_code_id: u64,
}

#[cw_serde]
pub struct PoolRecord {
    pub pool_id: u64,
    pub psp_pool: Addr,
    pub credit_manager: Addr,
    pub yield_distributor: Addr,
    pub yield_reserve: Addr,
    pub created_at: u64,
}

#[cw_serde]
pub struct PendingPoolCreation {
    pub pool_id: u64,
    pub label_prefix: String,
    pub created_at: u64,
    pub psp_pool_init_msg: Binary,
    pub credit_manager_init_msg: Binary,
    pub yield_distributor_init_msg: Binary,
    pub yield_reserve_init_msg: Binary,
    pub psp_pool: Option<Addr>,
    pub credit_manager: Option<Addr>,
    pub yield_distributor: Option<Addr>,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const NEXT_POOL_ID: Item<u64> = Item::new("next_pool_id");
pub const POOLS: Map<u64, PoolRecord> = Map::new("pools");
pub const PENDING_POOL_CREATION: Item<PendingPoolCreation> = Item::new("pending_pool_creation");

pub const CONTRACT_NAME: &str = "crates.io:defa-pool-factory";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const REPLY_ID_PSP_POOL: u64 = 1;
pub const REPLY_ID_CREDIT_MANAGER: u64 = 2;
pub const REPLY_ID_YIELD_DISTRIBUTOR: u64 = 3;
pub const REPLY_ID_YIELD_RESERVE: u64 = 4;

```

## psp-pool/src/contract.rs

```
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

```

## psp-pool/src/custom.rs

```
use cosmwasm_std::{CosmosMsg, StdResult, Binary, StdError};
use prost::Message;

/// ZigChain TokenFactory message structures using prost
/// These match the exact proto definitions from ZigChain

#[derive(Clone, PartialEq, Message)]
pub struct MsgCreateDenom {
    #[prost(string, tag = "1")]
    pub sender: String,
    #[prost(string, tag = "2")]
    pub subdenom: String,
    #[prost(string, optional, tag = "3")]
    pub minting_cap: Option<String>,
    #[prost(bool, tag = "4")]
    pub can_change_minting_cap: bool,
    #[prost(string, optional, tag = "5")]
    pub uri: Option<String>,
    #[prost(string, optional, tag = "6")]
    pub uri_hash: Option<String>,
    #[prost(string, optional, tag = "7")]
    pub description: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Coin {
    #[prost(string, tag = "1")]
    pub denom: String,
    #[prost(string, tag = "2")]
    pub amount: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct MsgMintAndSendTokens {
    #[prost(string, tag = "1")]
    pub signer: String,
    #[prost(message, optional, tag = "2")]
    pub token: Option<Coin>,
    #[prost(string, tag = "3")]
    pub recipient: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct MsgBurnTokens {
    #[prost(string, tag = "1")]
    pub signer: String,
    #[prost(message, optional, tag = "2")]
    pub token: Option<Coin>,
}

/// Create a new denom using ZigChain TokenFactory
/// Returns a Stargate CosmosMsg that can be added to Response
pub fn create_denom_msg(
    creator: String,
    subdenom: String,
    minting_cap: String,
    can_change_minting_cap: bool,
    uri: Option<String>,
    uri_hash: Option<String>,
    description: Option<String>,
) -> StdResult<CosmosMsg> {
    let msg = MsgCreateDenom {
        sender: creator,
        subdenom,
        minting_cap: Some(minting_cap),
        can_change_minting_cap,
        uri,
        uri_hash,
        description,
    };

    let mut buf = Vec::new();
    msg.encode(&mut buf)
        .map_err(|e| StdError::generic_err(format!("Failed to encode MsgCreateDenom: {}", e)))?;

    #[allow(deprecated)]
    Ok(CosmosMsg::Stargate {
        type_url: "/zigchain.factory.MsgCreateDenom".to_string(),
        value: Binary::from(buf),
    })
}

/// Mint tokens and send to recipient using TokenFactory
pub fn mint_and_send_tokens_msg(
    signer: String,
    denom: String,
    amount: String,
    recipient: String,
) -> StdResult<CosmosMsg> {
    let msg = MsgMintAndSendTokens {
        signer,
        token: Some(Coin { denom, amount }),
        recipient,
    };

    let mut buf = Vec::new();
    msg.encode(&mut buf)
        .map_err(|e| StdError::generic_err(format!("Failed to encode MsgMintAndSendTokens: {}", e)))?;

    #[allow(deprecated)]
    Ok(CosmosMsg::Stargate {
        type_url: "/zigchain.factory.MsgMintAndSendTokens".to_string(),
        value: Binary::from(buf),
    })
}

/// Burn tokens using TokenFactory
pub fn burn_tokens_msg(
    signer: String,
    denom: String,
    amount: String,
) -> StdResult<CosmosMsg> {
    let msg = MsgBurnTokens {
        signer,
        token: Some(Coin { denom, amount }),
    };

    let mut buf = Vec::new();
    msg.encode(&mut buf)
        .map_err(|e| StdError::generic_err(format!("Failed to encode MsgBurnTokens: {}", e)))?;

    #[allow(deprecated)]
    Ok(CosmosMsg::Stargate {
        type_url: "/zigchain.factory.MsgBurnTokens".to_string(),
        value: Binary::from(buf),
    })
}

```

## psp-pool/src/error.rs

```

use cosmwasm_std::{StdError, Uint128};
use thiserror::Error;

/// Custom errors for the token vault contract
#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized: caller={caller}, expected={expected}")]
    Unauthorized { caller: String, expected: String },

    #[error("Unauthorized - only the configured CreditManager can perform this action")]
    CreditManagerOnly {},

    #[error("CreditManager address is not configured")]
    CreditManagerNotConfigured {},

    #[error("Invalid state transition - expected {expected}, got {got}")]
    InvalidState { expected: String, got: String },

    #[error("Contract is paused")]
    ContractPaused {},

    #[error("Invalid zero amount - amount must be greater than zero")]
    InvalidZeroAmount {},

    #[error("Insufficient LP balance - user does not have enough LP tokens to withdraw")]
    InsufficientLpBalance {},

    #[error("Insufficient vault balance - vault does not have enough stablecoins")]
    InsufficientVaultBalance {},

    #[error("Invalid subdenom - {reason}")]
    InvalidSubdenom { reason: String },

    #[error("Invalid minting cap - must be greater than zero")]
    InvalidMintingCap {},

    #[error("No stablecoin sent - must send stablecoin via info.funds")]
    NoStablecoinSent {},

    #[error("No LP tokens sent - must send LP tokens via info.funds")]
    NoLpTokensSent {},

    #[error("Overflow error during {operation}")]
    OverflowError { operation: String },

    #[error("Invalid address - {0}")]
    InvalidAddress(String),

    #[error("Vault state inconsistent - total deposited does not match LP supply")]
    InconsistentVaultState {},

    #[error("Pending withdrawal not found")]
    WithdrawalNotFound {},

    #[error("Withdrawal still locked - cannot claim until {release_time}")]
    WithdrawalLocked { release_time: u64 },

    #[error("Withdrawal requests are only allowed on Monday (current day index: {current_day})")]
    WithdrawalWindowClosed { current_day: u64 },

    #[error("FIFO queue violation: head_queue_id={head_queue_id}, requested_queue_id={requested_queue_id}")]
    QueueOrderViolation {
        head_queue_id: u64,
        requested_queue_id: u64,
    },

    #[error("Queue metadata not found for queue_id={queue_id}")]
    QueueEntryNotFound { queue_id: u64 },

    #[error("No pending withdrawals found for user")]
    NoPendingWithdrawals {},

    #[error("No funds should be sent with this message")]
    UnexpectedFunds {},

    #[error("Invalid withdrawal delay - must be between {min} and {max} seconds")]
    InvalidWithdrawalDelay { min: u64, max: u64 },

    #[error("Invalid vault value update - new total ({new_total}) must be >= current total ({current_total})")]
    InvalidVaultValueDecrease {
        current_total: Uint128,
        new_total: Uint128,
    },

    #[error("Insufficient contract balance - contract has {available} but needs {required}")]
    InsufficientContractBalance {
        available: Uint128,
        required: Uint128,
    },

    #[error("Invalid pagination limit")]
    InvalidPaginationLimit {},

    #[error("Backfill cap exceeded: cap={cap}, requested={requested}")]
    BackfillCapExceeded { cap: Uint128, requested: Uint128 },
}

```

## psp-pool/src/lib.rs

```
pub mod contract;
pub mod custom;
mod error;
pub mod msg;
pub mod state;

pub use crate::error::ContractError;

#[cfg(test)]
pub mod test;

```

## psp-pool/src/msg.rs

```
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Timestamp, Uint128};

/// Message sent when instantiating the contract
#[cw_serde]
pub struct InstantiateMsg {
    /// Denom of the stablecoin (e.g., "uzig" or existing TokenFactory denom)
    pub stablecoin_denom: String,
    /// Subdenom for the LP token (will be created as coin.{contract}.{subdenom})
    pub lp_subdenom: String,
    /// Maximum supply cap for LP tokens
    pub lp_minting_cap: Uint128,
    /// Can the minting cap be changed later
    pub can_change_minting_cap: Option<bool>,
    /// Optional metadata URI for LP token
    pub uri: Option<String>,
    /// Optional URI hash for LP token metadata
    pub uri_hash: Option<String>,
    /// Optional description for LP token
    pub description: Option<String>,
    /// Optional admin address (defaults to sender if not provided)
    pub admin: Option<String>,
    /// Withdrawal delay in seconds (IMMUTABLE after deployment) - REQUIRED
    /// Must be between 120 (2 minutes) and 2,592,000 (30 days)
    /// Choose carefully as this cannot be changed after instantiation!
    pub withdrawal_delay_seconds: u64,
    /// Investor APY in basis points (bps)
    pub investor_apy_bps: u64,
    /// YieldReserve contract address
    pub yield_reserve: String,
}

/// Messages that can be executed on the contract
#[cw_serde]
pub enum ExecuteMsg {
    /// Deposit stablecoins to receive LP tokens
    /// User must send stablecoins via info.funds
    Deposit {},
    /// Request a withdrawal by burning LP tokens
    /// Creates a pending withdrawal with a time lock
    /// User must send LP tokens via info.funds
    RequestWithdraw {},
    /// Claim a pending withdrawal after the time lock expires
    ClaimWithdraw {
        /// ID of the withdrawal request to claim
        withdrawal_id: u64,
    },
    /// Update contract configuration (admin only)
    UpdateConfig {
        /// New stablecoin denom (optional)
        stablecoin_denom: Option<String>,
        /// New admin address (optional)
        admin: Option<String>,
    },
    /// Admin-only: Withdraw stablecoin from vault to admin's wallet
    /// This allows admin to manage funds externally for yield generation
    /// Does not change vault accounting (funds still tracked in total_deposited)
    /// Admin is responsible for depositing funds + yield back later
    AdminWithdraw {
        /// Amount of stablecoin to withdraw
        amount: Uint128,
    },
    /// Admin-only: Return principal and deposit yield to vault
    /// Admin sends total funds (principal + yield) via info.funds
    /// Contract separates them correctly:
    /// - Principal is returned without changing total_deposited (no price impact)
    /// - Only yield is added to total_deposited (increases price per share)
    /// This ensures accurate accounting and proportional yield distribution
    AdminDepositYield {
        /// Amount of principal being returned (does NOT increase total_deposited)
        principal_amount: Uint128,
        /// Amount of yield earned (DOES increase total_deposited)
        yield_amount: Uint128,
    },
    /// Admin-only: transition facility from Fundraising to Active.
    ExecuteFacility {},
    /// Transition facility from Active to WindingDown.
    /// Callable by anyone once off-chain tenure checks pass.
    ExpireFacility {},
    /// Admin-only: transition facility from WindingDown to Settled.
    MarkSettled {},
    /// Admin-only: transition facility from Settled to Closed.
    CloseFacility {},
    /// Admin-only: set the authorized CreditManager address.
    SetCreditManager {
        credit_manager: String,
    },
    /// CreditManager-only: disburse principal to PSP borrower.
    DisburseToPsp {
        amount: Uint128,
        recipient: String,
    },
    /// Permissionless holder-index sync for TokenFactory LP balances.
    /// Required because native bank balances are not enumerable by denom.
    SyncLpHolder {
        address: String,
    },
    /// Admin-only: pause all mutating operations.
    EmergencyPause {},
    /// Admin-only: unpause mutating operations.
    EmergencyUnpause {},
}

/// Query messages
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Get contract configuration
    #[returns(ConfigResponse)]
    Config {},

    /// Get vault information (total deposits, LP supply, etc.)
    #[returns(VaultInfoResponse)]
    VaultInfo {},

    /// Get user's deposit information
    #[returns(UserInfoResponse)]
    UserInfo {
        /// User address to query
        address: String,
    },

    /// Get all pending withdrawals for a user
    #[returns(PendingWithdrawalsResponse)]
    PendingWithdrawals {
        /// User address to query
        address: String,
    },

    /// Get a specific pending withdrawal
    #[returns(WithdrawalResponse)]
    Withdrawal {
        /// User address to query
        address: String,
        /// Withdrawal ID
        withdrawal_id: u64,
    },

    /// Get pool state
    #[returns(PoolStateResponse)]
    PoolState {},

    /// Queue health signal used by CreditManager drawdown guards.
    #[returns(OldestQueueWaitDaysResponse)]
    OldestQueueWaitDays {},

    /// Paginated list of indexed LP holder addresses.
    #[returns(LpHoldersResponse)]
    LpHolders {
        start_after: Option<String>,
        limit: Option<u32>,
    },
}

/// Response for Config query
#[cw_serde]
pub struct ConfigResponse {
    /// Denom of the stablecoin
    pub stablecoin_denom: String,
    /// Full denom of the LP token
    pub lp_full_denom: String,
    /// Admin address
    pub admin: Addr,
    /// Withdrawal delay in seconds
    pub withdrawal_delay: u64,
    /// Authorized CreditManager contract that can disburse pool principal.
    pub credit_manager: Option<String>,
}

/// Response for VaultInfo query
#[cw_serde]
pub struct VaultInfoResponse {
    /// Total stablecoin value in the vault (includes accrued yield)
    /// This is the accounting total managed by admin
    pub total_deposited: Uint128,
    /// Total LP tokens in circulation
    pub total_lp_supply: Uint128,
    /// Total amount locked in pending withdrawals
    pub total_pending_withdrawals: Uint128,
    /// Current price per share (in stablecoin base units)
    /// Calculated as: total_deposited / total_lp_supply
    pub price_per_share: String,
}

/// Response for UserInfo query
#[cw_serde]
pub struct UserInfoResponse {
    /// User's address
    pub address: Addr,
    /// Amount of LP tokens owned by user
    pub lp_balance: Uint128,
    /// Equivalent stablecoin value
    pub stablecoin_value: Uint128,
    /// Timestamp of first deposit (for pro-rata yield)
    pub deposit_timestamp: u64,
}

/// Information about a single pending withdrawal
#[cw_serde]
pub struct WithdrawalInfo {
    /// Withdrawal ID
    pub id: u64,
    /// FIFO queue ID assigned at request time
    pub queue_id: u64,
    /// Amount of stablecoins to be withdrawn
    pub amount: Uint128,
    /// Request creation time
    pub requested_at: Timestamp,
    /// Time when the withdrawal can be claimed
    pub release_time: Timestamp,
    /// Whether the withdrawal is claimable now
    pub claimable: bool,
}

/// Response for PendingWithdrawals query
#[cw_serde]
pub struct PendingWithdrawalsResponse {
    /// User's address
    pub address: Addr,
    /// List of pending withdrawals
    pub withdrawals: Vec<WithdrawalInfo>,
}

/// Response for Withdrawal query
#[cw_serde]
pub struct WithdrawalResponse {
    /// User's address
    pub address: Addr,
    /// Withdrawal information
    pub withdrawal: WithdrawalInfo,
}

/// Response for PoolState query.
#[cw_serde]
pub struct PoolStateResponse {
    pub state: String,
    pub paused: bool,
}

#[cw_serde]
pub struct OldestQueueWaitDaysResponse {
    pub days: Option<u64>,
}

#[cw_serde]
pub struct LpHoldersResponse {
    pub holders: Vec<String>,
}

/// Migration message (for future upgrades)
#[cw_serde]
pub struct MigrateMsg {}

```

## psp-pool/src/state.rs

```
/// Per-user info for LP holders (for pro-rata yield logic)
#[cw_serde]
pub struct UserInfo {
    pub deposit_timestamp: u64,
    pub lp_balance: Uint128,
}

/// Map of user address to UserInfo
pub const USER_INFOS: Map<&Addr, UserInfo> = Map::new("user_infos");
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Timestamp, Uint128};
use cw_storage_plus::{Item, Map};
use defa_types::{PoolState, SECONDS_PER_DAY};

/// Minimum withdrawal delay: 2 minutes (120 seconds)
/// Provides flexibility for testing while maintaining security
pub const MIN_WITHDRAWAL_DELAY: u64 = 120;
/// Maximum withdrawal delay: 30 days (2,592,000 seconds)
pub const MAX_WITHDRAWAL_DELAY: u64 = 2_592_000;

/// Contract configuration stored in state
#[cw_serde]
pub struct Config {
    /// Denom of the stablecoin (e.g., "uzig" or TokenFactory denom)
    pub stablecoin_denom: String,
    /// Full denom of the LP token created by this contract
    /// Format: coin.{contract_address}.{subdenom}
    pub lp_full_denom: String,
    /// Admin address that can update config and manage vault funds
    /// Admin has exclusive rights to move funds to yield protocols
    pub admin: Addr,
    /// Withdrawal delay in seconds - IMMUTABLE after instantiation
    /// This provides security against flash attacks and unauthorized withdrawals
    pub withdrawal_delay: u64,
    /// Authorized CreditManager allowed to disburse principal to PSP.
    pub credit_manager: Option<Addr>,
    /// Investor APY in basis points (bps)
    pub investor_apy_bps: u64,
    /// YieldReserve contract address
    pub yield_reserve: Addr,
    /// Execution amount cap for backfill ceiling
    pub execution_amount: Uint128,
}

/// Vault statistics
#[cw_serde]
pub struct VaultState {
    /// Total stablecoin value owned by the vault (accounting value)
    /// This includes all deposited funds plus accrued yield
    /// Admin manages actual fund movements; this tracks the accounting total
    /// Formula: total_deposited = deposits + yield - withdrawals
    pub total_deposited: Uint128,
    /// Total LP tokens minted by this vault
    pub total_lp_minted: Uint128,
    /// Total amount of stablecoins locked in pending withdrawals
    /// (valued at the time of withdrawal request)
    pub total_pending_withdrawals: Uint128,
}

/// Pending withdrawal request
#[cw_serde]
pub struct PendingWithdrawal {
    /// FIFO queue ID assigned when request is created
    pub queue_id: u64,
    /// Amount of stablecoins to be withdrawn
    pub amount: Uint128,
    /// Request timestamp
    pub requested_at: Timestamp,
    /// Time when the withdrawal can be claimed
    pub release_time: Timestamp,
}

#[cw_serde]
pub struct QueueEntry {
    pub user: Addr,
    pub withdrawal_id: u64,
    pub requested_at: Timestamp,
}

/// Counter for generating unique withdrawal IDs per user
#[cw_serde]
pub struct WithdrawalCounter {
    pub next_id: u64,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const VAULT_STATE: Item<VaultState> = Item::new("vault_state");

/// Map of pending withdrawals: (user_address, withdrawal_id) -> PendingWithdrawal
pub const PENDING_WITHDRAWALS: Map<(&Addr, u64), PendingWithdrawal> =
    Map::new("pending_withdrawals");

/// Counter for each user's next withdrawal ID
pub const WITHDRAWAL_COUNTERS: Map<&Addr, WithdrawalCounter> = Map::new("withdrawal_counters");

/// Queue head pointer (next FIFO queue_id claimable when conditions are met).
pub const WITHDRAWAL_QUEUE_HEAD: Item<u64> = Item::new("withdrawal_queue_head");

/// Monotonic queue ID allocation pointer.
pub const WITHDRAWAL_QUEUE_NEXT: Item<u64> = Item::new("withdrawal_queue_next");

/// queue_id -> queue entry metadata.
pub const WITHDRAWAL_QUEUE: Map<u64, QueueEntry> = Map::new("withdrawal_queue");

/// (user, withdrawal_id) -> queue_id mapping for O(1) claim lookup.
pub const USER_WITHDRAWAL_QUEUE_ID: Map<(&Addr, u64), u64> =
    Map::new("user_withdrawal_queue_id");

/// Indexed LP holders used for on-chain yield payout enumeration.
pub const LP_HOLDERS: Map<&Addr, bool> = Map::new("lp_holders");

/// Current lifecycle state for this facility.
pub const POOL_STATE: Item<PoolState> = Item::new("pool_state");

/// Emergency pause flag. When true, all mutating user/admin operations are blocked.
pub const PAUSED: Item<bool> = Item::new("paused");

pub const CONTRACT_NAME: &str = "crates.io:defa-psp-pool";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const MONDAY_WITHDRAWAL_WINDOW_DAY: u64 = 1;

pub fn day_of_week(timestamp_secs: u64) -> u64 {
    // Unix epoch started on Thursday (index 4 in [Sun=0..Sat=6]).
    let unix_epoch_day_of_week_offset = 4u64;
    let days_since_epoch = timestamp_secs / SECONDS_PER_DAY;
    (days_since_epoch + unix_epoch_day_of_week_offset) % 7
}

```

## psp-pool/src/test.rs

```

use cosmwasm_std::Addr;
use crate::contract::{execute, instantiate, query};
use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, OldestQueueWaitDaysResponse,
    PoolStateResponse, QueryMsg, VaultInfoResponse,
};
use crate::state::{day_of_week, MONDAY_WITHDRAWAL_WINDOW_DAY, PENDING_WITHDRAWALS};

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, message_info};
    use cosmwasm_std::{coins, from_json, Uint128};

    fn move_to_monday(env: &mut cosmwasm_std::Env) {
        while day_of_week(env.block.time.seconds()) != MONDAY_WITHDRAWAL_WINDOW_DAY {
            env.block.time = env.block.time.plus_seconds(86_400);
        }
    }

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
            investor_apy_bps: 500,
            yield_reserve: "wasm1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqp4d5g9".to_string(),
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(1, res.messages.len());

        let res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
        let config: ConfigResponse = from_json(&res).unwrap();
        assert_eq!("uzig", config.stablecoin_denom);
        assert!(config.lp_full_denom.contains("lplp"));
        assert_eq!(config.admin, Addr::unchecked("creator"));
        assert_eq!(172_800, config.withdrawal_delay);
        assert_eq!(None, config.credit_manager);
    }

    #[test]
    fn test_disburse_to_psp_requires_credit_manager_wiring() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let credit_manager =
            cosmwasm_std::testing::MockApi::default().addr_make("credit-manager").to_string();
        let psp = cosmwasm_std::testing::MockApi::default()
            .addr_make("psp-borrower")
            .to_string();

        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: Some(false),
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120,
            investor_apy_bps: 500,
            yield_reserve: "yieldreserve".to_string(),
        };
        instantiate(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked("creator"), &[]),
            msg,
        )
        .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(credit_manager.clone()), &[]),
            ExecuteMsg::DisburseToPsp {
                amount: Uint128::new(100),
                recipient: psp.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::CreditManagerNotConfigured {}));

        execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked("creator"), &[]),
            ExecuteMsg::SetCreditManager {
                credit_manager: credit_manager.clone(),
            },
        )
        .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(credit_manager.clone()), &[]),
            ExecuteMsg::DisburseToPsp {
                amount: Uint128::new(100),
                recipient: psp.clone(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::InvalidState { .. }));

        execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked("creator"), &[]),
            ExecuteMsg::ExecuteFacility {},
        )
        .unwrap();

        deps.querier
            .bank
            .update_balance(env.contract.address.to_string(), coins(500, "uzig"));

        let err = execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked("intruder"), &[]),
            ExecuteMsg::DisburseToPsp {
                amount: Uint128::new(100),
                recipient: "psp".to_string(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::CreditManagerOnly {}));

        let res = execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked(credit_manager), &[]),
            ExecuteMsg::DisburseToPsp {
                amount: Uint128::new(200),
                recipient: psp,
            },
        )
        .unwrap();
        assert_eq!(1, res.messages.len());
    }

    #[test]
    fn test_lifecycle_transitions_to_closed() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: Some(false),
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120,
            investor_apy_bps: 500,
            yield_reserve: "yieldreserve".to_string(),
        };
        instantiate(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked("creator"), &[]),
            msg,
        )
        .unwrap();

        execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked("creator"), &[]),
            ExecuteMsg::ExecuteFacility {},
        )
        .unwrap();

        let err = execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked("creator"), &[]),
            ExecuteMsg::MarkSettled {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::InvalidState { .. }));

        execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked("keeper"), &[]),
            ExecuteMsg::ExpireFacility {},
        )
        .unwrap();

        let state_bin = query(deps.as_ref(), env.clone(), QueryMsg::PoolState {}).unwrap();
        let state: PoolStateResponse = from_json(&state_bin).unwrap();
        assert_eq!(state.state, "WindingDown");

        execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked("creator"), &[]),
            ExecuteMsg::MarkSettled {},
        )
        .unwrap();

        let state_bin = query(deps.as_ref(), env.clone(), QueryMsg::PoolState {}).unwrap();
        let state: PoolStateResponse = from_json(&state_bin).unwrap();
        assert_eq!(state.state, "Settled");

        execute(
            deps.as_mut(),
            env.clone(),
            message_info(&Addr::unchecked("creator"), &[]),
            ExecuteMsg::CloseFacility {},
        )
        .unwrap();

        let state_bin = query(deps.as_ref(), env, QueryMsg::PoolState {}).unwrap();
        let state: PoolStateResponse = from_json(&state_bin).unwrap();
        assert_eq!(state.state, "Closed");
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
            investor_apy_bps: 500,
            yield_reserve: "yieldreserve".to_string(),
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
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
            investor_apy_bps: 500,
            yield_reserve: "yieldreserve".to_string(),
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
            investor_apy_bps: 500,
            yield_reserve: "yieldreserve".to_string(),
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
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
            investor_apy_bps: 500,
            yield_reserve: "yieldreserve".to_string(),
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
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
            investor_apy_bps: 500,
            yield_reserve: "yieldreserve".to_string(),
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
            investor_apy_bps: 500,
            yield_reserve: "yieldreserve".to_string(),
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
            investor_apy_bps: 500,
            yield_reserve: "yieldreserve".to_string(),
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
            investor_apy_bps: 500,
            yield_reserve: "yieldreserve".to_string(),
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
        instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

        // Get the LP denom
        let config_res = query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap();
        let config: ConfigResponse = from_json(&config_res).unwrap();
        let lp_denom = config.lp_full_denom;
        assert_eq!(custom_delay, config.withdrawal_delay);


        // Test deposit (simulated - in real chain TokenFactory would mint)
        let info = message_info(&Addr::unchecked("user"), &coins(1000, "uzig"));
        let res = execute(deps.as_mut(), env.clone(), info, ExecuteMsg::Deposit {}).unwrap();
        assert_eq!(1, res.messages.len());

        // Set pool state to Active to allow withdrawals
        crate::state::POOL_STATE.save(deps.as_mut().storage, &defa_types::PoolState::Active).unwrap();

        move_to_monday(&mut env);

        // Test request withdrawal
        let info = message_info(&Addr::unchecked("user"), &coins(500, &lp_denom));
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
        assert_eq!(0, pending_withdrawal.queue_id);
        assert_eq!(Uint128::new(500), pending_withdrawal.amount);
        assert!(env.block.time < pending_withdrawal.release_time);

        // Try to claim too early
        let info = message_info(&Addr::unchecked("user"), &[]);
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

        // Try to claim - will fail because contract has no stablecoin balance
        // In production, admin would ensure liquidity by keeping funds in contract
        // or bringing them back from yield protocols before users claim
        let err = execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ClaimWithdraw { withdrawal_id: 0 },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::InsufficientContractBalance { .. }
        ));

        // Verify withdrawal still exists (not claimed yet)
        let withdrawal_exists = PENDING_WITHDRAWALS
            .may_load(deps.as_ref().storage, (&user_addr, 0))
            .unwrap();
        assert!(withdrawal_exists.is_some());

        // Verify vault state still shows pending withdrawal
        let vault_info_res = query(deps.as_ref(), env.clone(), QueryMsg::VaultInfo {}).unwrap();
        let vault_info: VaultInfoResponse = from_json(&vault_info_res).unwrap();
        assert_eq!(Uint128::new(500), vault_info.total_pending_withdrawals);
        // Total deposited stays at 1000 because withdrawal hasn't been paid out yet
        assert_eq!(Uint128::new(1000), vault_info.total_deposited);
    }

    #[test]
    fn test_request_withdraw_requires_monday_and_queue_age_query() {
        let mut deps = mock_dependencies();
        let mut env = mock_env();

        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: Some(false),
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 7200,
            investor_apy_bps: 500,
            yield_reserve: "yieldreserve".to_string(),
        };

        let info = message_info(&Addr::unchecked("creator"), &[]);
        instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

        let config_res = query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap();
        let config: ConfigResponse = from_json(&config_res).unwrap();


        let deposit_info = message_info(&Addr::unchecked("user"), &coins(1000, "uzig"));
        execute(
            deps.as_mut(),
            env.clone(),
            deposit_info,
            ExecuteMsg::Deposit {},
        )
        .unwrap();

        // Set pool state to Active to allow withdrawals
        crate::state::POOL_STATE.save(deps.as_mut().storage, &defa_types::PoolState::Active).unwrap();

        let non_monday_req = message_info(&Addr::unchecked("user"), &coins(500, &config.lp_full_denom));
        let err = execute(
            deps.as_mut(),
            env.clone(),
            non_monday_req,
            ExecuteMsg::RequestWithdraw {},
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::WithdrawalWindowClosed { .. }));

        move_to_monday(&mut env);

        let monday_req = message_info(&Addr::unchecked("user"), &coins(500, &config.lp_full_denom));
        execute(
            deps.as_mut(),
            env.clone(),
            monday_req,
            ExecuteMsg::RequestWithdraw {},
        )
        .unwrap();

        let wait_days_bin = query(deps.as_ref(), env.clone(), QueryMsg::OldestQueueWaitDays {}).unwrap();
        let wait_days: OldestQueueWaitDaysResponse = from_json(&wait_days_bin).unwrap();
        assert_eq!(wait_days.days, Some(0));

        env.block.time = env.block.time.plus_seconds(2 * 86_400);
        let wait_days_bin = query(deps.as_ref(), env, QueryMsg::OldestQueueWaitDays {}).unwrap();
        let wait_days: OldestQueueWaitDaysResponse = from_json(&wait_days_bin).unwrap();
        assert_eq!(wait_days.days, Some(2));
    }

    #[test]
    fn test_adversarial_rounding_deposit() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: None,
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120,
            investor_apy_bps: 500,
            yield_reserve: "yieldreserve".to_string(),
        };
        instantiate(deps.as_mut(), env.clone(), message_info(&Addr::unchecked("creator"), &[]), msg).unwrap();

        // 1. Initial deposit of 1 unit to set a baseline
        execute(deps.as_mut(), env.clone(), message_info(&Addr::unchecked("user1"), &coins(1, "uzig")), ExecuteMsg::Deposit {}).unwrap();

        // 2. Admin adds yield to inflate share price (1 uzig -> 1000 uzig value)
        // Price = 1000 / 1 = 1000 uzig per share
        execute(deps.as_mut(), env.clone(), message_info(&Addr::unchecked("creator"), &coins(999, "uzig")), ExecuteMsg::AdminDepositYield {
            principal_amount: Uint128::zero(),
            yield_amount: Uint128::new(999),
        }).unwrap();

        // 3. Rounding Attack: User deposits 999 uzig. 
        // Shares = amount / price = 999 / 1000 = 0.999 -> rounded down to 0
        let user2_info = message_info(&Addr::unchecked("user2"), &coins(999, "uzig"));
        let err = execute(deps.as_mut(), env.clone(), user2_info, ExecuteMsg::Deposit {}).unwrap_err();
        
        // Contract should reject zero-share deposits to prevent loss of funds
        // In this implementation, it returns a StdError with a specific message
        match err {
            ContractError::Std(e) => assert!(e.to_string().contains("Deposit too small")),
            _ => panic!("Expected StdError with 'Deposit too small', got {:?}", err),
        }
    }

    #[test]
    fn test_admin_yield_negative_scenario() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: None,
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120,
            investor_apy_bps: 500,
            yield_reserve: "yieldreserve".to_string(),
        };
        instantiate(deps.as_mut(), env.clone(), message_info(&Addr::unchecked("creator"), &[]), msg).unwrap();

        // 1. Initial deposit of 1000 units
        execute(deps.as_mut(), env.clone(), message_info(&Addr::unchecked("user1"), &coins(1000, "uzig")), ExecuteMsg::Deposit {}).unwrap();

        // Admin returns principal (previously withdrawn) but adds 500 yield
        let admin_info = message_info(&Addr::unchecked("creator"), &coins(1500, "uzig"));
        execute(
            deps.as_mut(),
            env.clone(),
            admin_info,
            ExecuteMsg::AdminDepositYield {
                principal_amount: Uint128::new(1000),
                yield_amount: Uint128::new(500),
            },
        ).unwrap();

        let vault_info: VaultInfoResponse = from_json(query(deps.as_ref(), env.clone(), QueryMsg::VaultInfo {}).unwrap()).unwrap();
        // Total deposited should be 1000 (orig) + 500 (yield) = 1500
        assert_eq!(Uint128::new(1500), vault_info.total_deposited);
        assert_eq!("1.500000", vault_info.price_per_share);
    }

    #[test]
    fn test_admin_deposit_yield_success() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        // Initialize contract (admin defaults to creator)
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: Some(false),
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120,
            investor_apy_bps: 500,
            yield_reserve: "yieldreserve".to_string(),
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
        instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

        // User deposits 1000 uzig
        let user_info = message_info(&Addr::unchecked("user"), &coins(1000, "uzig"));
        execute(deps.as_mut(), env.clone(), user_info, ExecuteMsg::Deposit {}).unwrap();

        // Check initial vault state
        let vault_info = query(deps.as_ref(), env.clone(), QueryMsg::VaultInfo {}).unwrap();
        let vault: VaultInfoResponse = from_json(&vault_info).unwrap();
        assert_eq!(Uint128::new(1000), vault.total_deposited);
        assert_eq!(Uint128::new(1000), vault.total_lp_supply);
        assert_eq!("1.000000", vault.price_per_share); // Initial 1:1

        // Admin (creator) deposits 100 uzig as pure yield (no principal return)
        let admin_info = message_info(&Addr::unchecked("creator"), &coins(100, "uzig"));
        let res = execute(
            deps.as_mut(),
            env.clone(),
            admin_info,
            ExecuteMsg::AdminDepositYield {
                principal_amount: Uint128::zero(),
                yield_amount: Uint128::new(100),
            },
        )
        .unwrap();

        // Verify response attributes
        assert_eq!("admin_deposit_yield", res.attributes[0].value);
        assert_eq!("0", res.attributes[2].value); // principal_returned
        assert_eq!("100", res.attributes[3].value); // yield_deposited
        assert_eq!("100", res.attributes[4].value); // total_received
        assert_eq!("1000", res.attributes[5].value); // old_total_deposited
        assert_eq!("1100", res.attributes[6].value); // new_total_deposited
        assert_eq!("1.100000", res.attributes[7].value); // price_per_share

        // Verify vault state updated correctly
        let vault_info = query(deps.as_ref(), env.clone(), QueryMsg::VaultInfo {}).unwrap();
        let vault: VaultInfoResponse = from_json(&vault_info).unwrap();
        assert_eq!(Uint128::new(1100), vault.total_deposited); // Increased by yield
        assert_eq!(Uint128::new(1000), vault.total_lp_supply); // LP supply unchanged
        assert_eq!("1.100000", vault.price_per_share); // 10% increase
    }

    #[test]
    fn test_admin_deposit_yield_unauthorized() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        // Initialize (admin defaults to creator)
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: Some(false),
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120,
            investor_apy_bps: 500,
            yield_reserve: "yieldreserve".to_string(),
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
        instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

        // Try to deposit yield as non-admin
        let non_admin_info = message_info(&Addr::unchecked("attacker"), &coins(100, "uzig"));
        let err = execute(
            deps.as_mut(),
            env,
            non_admin_info,
            ExecuteMsg::AdminDepositYield {
                principal_amount: Uint128::zero(),
                yield_amount: Uint128::new(100),
            },
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::Unauthorized { .. }));
    }

    #[test]
    fn test_admin_deposit_yield_wrong_denom() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        // Initialize (admin defaults to creator)
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: Some(false),
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120,
            investor_apy_bps: 500,
            yield_reserve: "yieldreserve".to_string(),
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
        instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

        // Try to deposit wrong token as admin (creator)
        let admin_info = message_info(&Addr::unchecked("creator"), &coins(100, "wrongtoken"));
        let err = execute(
            deps.as_mut(),
            env,
            admin_info,
            ExecuteMsg::AdminDepositYield {
                principal_amount: Uint128::zero(),
                yield_amount: Uint128::new(100),
            },
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::NoStablecoinSent {}));
    }

    #[test]
    fn test_admin_deposit_yield_zero_amount() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        // Initialize (admin defaults to creator)
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: Some(false),
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120,
            investor_apy_bps: 500,
            yield_reserve: "yieldreserve".to_string(),
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
        instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

        // Try to deposit zero amount as admin (creator)
        let admin_info = message_info(&Addr::unchecked("creator"), &coins(0, "uzig"));
        let err = execute(
            deps.as_mut(),
            env,
            admin_info,
            ExecuteMsg::AdminDepositYield {
                principal_amount: Uint128::zero(),
                yield_amount: Uint128::zero(),
            },
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::InvalidZeroAmount {}));
    }

    #[test]
    fn test_admin_deposit_yield_multiple_deposits() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        // Initialize (admin defaults to creator)
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: Some(false),
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120,
            investor_apy_bps: 500,
            yield_reserve: "yieldreserve".to_string(),
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
        instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

        // User deposits 1000 uzig
        let user_info = message_info(&Addr::unchecked("user"), &coins(1000, "uzig"));
        execute(deps.as_mut(), env.clone(), user_info, ExecuteMsg::Deposit {}).unwrap();

        // Admin (creator) deposits yield multiple times
        let admin_info = message_info(&Addr::unchecked("creator"), &coins(50, "uzig"));
        execute(
            deps.as_mut(),
            env.clone(),
            admin_info.clone(),
            ExecuteMsg::AdminDepositYield {
                principal_amount: Uint128::zero(),
                yield_amount: Uint128::new(50),
            },
        )
        .unwrap();

        // Check after first deposit: 1000 + 50 = 1050
        let vault_info = query(deps.as_ref(), env.clone(), QueryMsg::VaultInfo {}).unwrap();
        let vault: VaultInfoResponse = from_json(&vault_info).unwrap();
        assert_eq!(Uint128::new(1050), vault.total_deposited);
        assert_eq!("1.050000", vault.price_per_share);

        // Second yield deposit by admin (creator)
        let admin_info2 = message_info(&Addr::unchecked("creator"), &coins(100, "uzig"));
        execute(
            deps.as_mut(),
            env.clone(),
            admin_info2,
            ExecuteMsg::AdminDepositYield {
                principal_amount: Uint128::zero(),
                yield_amount: Uint128::new(100),
            },
        )
        .unwrap();

        // Check after second deposit: 1050 + 100 = 1150
        let vault_info = query(deps.as_ref(), env.clone(), QueryMsg::VaultInfo {}).unwrap();
        let vault: VaultInfoResponse = from_json(&vault_info).unwrap();
        assert_eq!(Uint128::new(1150), vault.total_deposited);
        assert_eq!("1.150000", vault.price_per_share); // 15% total yield

        // Verify LP supply unchanged
        assert_eq!(Uint128::new(1000), vault.total_lp_supply);
    }

    #[test]
    fn test_admin_deposit_yield_no_lp_holders() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        // Initialize (admin defaults to creator)
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: Some(false),
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120,
            investor_apy_bps: 500,
            yield_reserve: "yieldreserve".to_string(),
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
        instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

        // Admin (creator) tries to deposit yield when no users have deposited yet
        let admin_info = message_info(&Addr::unchecked("creator"), &coins(100, "uzig"));
        let res = execute(
            deps.as_mut(),
            env.clone(),
            admin_info,
            ExecuteMsg::AdminDepositYield {
                principal_amount: Uint128::zero(),
                yield_amount: Uint128::new(100),
            },
        )
        .unwrap();

        // Should succeed - total_deposited increases
        assert_eq!("admin_deposit_yield", res.attributes[0].value);
        assert_eq!("0", res.attributes[2].value); // principal_returned
        assert_eq!("100", res.attributes[3].value); // yield_deposited

        // Verify vault state
        let vault_info = query(deps.as_ref(), env, QueryMsg::VaultInfo {}).unwrap();
        let vault: VaultInfoResponse = from_json(&vault_info).unwrap();
        assert_eq!(Uint128::new(100), vault.total_deposited);
        assert_eq!(Uint128::new(0), vault.total_lp_supply); // No LP tokens
        assert_eq!("1.0", vault.price_per_share); // Default when no LP
    }
}

```

## yield-distributor/src/contract.rs

```
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

```

## yield-distributor/src/error.rs

```
use cosmwasm_std::{StdError, Uint128};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized: caller={caller}, expected={expected}")]
    Unauthorized { caller: String, expected: String },

    #[error("Invalid zero amount")]
    InvalidZeroAmount {},

    #[error("Investor APY must be greater than zero")]
    InvalidInvestorApy {},

    #[error("Cycle days must be greater than zero")]
    InvalidCycleDays {},

    #[error("Payout sum mismatch: expected={expected}, got={actual}")]
    PayoutSumMismatch { expected: Uint128, actual: Uint128 },

    #[error("LP supply is zero; cannot disburse yield")]
    NoLpSupply {},

    #[error("No indexed LP holders found in PSPPool")]
    NoIndexedLpHolders {},

    #[error("Invalid pagination limit")]
    InvalidPaginationLimit {},

    #[error("Could not allocate remainder to any holder")]
    RemainderAllocationFailed {},

    #[error("Disbursement attempted before cycle boundary. next_cycle_time={next_cycle_time}, now={now}")]
    CycleNotReady { next_cycle_time: u64, now: u64 },

    #[error("No funds should be sent with this message")]
    UnexpectedFunds {},

    #[error("Arithmetic overflow while computing {operation}")]
    ArithmeticOverflow { operation: String },

    #[error("Insufficient YieldReserve balance: required={required}, available={available}")]
    InsufficientYieldReserve { required: Uint128, available: Uint128 },
}

```

## yield-distributor/src/lib.rs

```
pub mod contract;
mod error;
pub mod msg;
pub mod state;

pub use crate::error::ContractError;

```

## yield-distributor/src/msg.rs

```
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;

#[cw_serde]
pub struct InstantiateMsg {
    pub admin: String,
    pub psp_pool: String,
    pub yield_reserve: String,
    pub investor_apy_bps: u64,
    pub cycle_days: u64,
    pub cycle_anchor_time: Option<u64>,
}

#[cw_serde]
pub enum ExecuteMsg {
    UpdateWiring {
        psp_pool: Option<String>,
        yield_reserve: Option<String>,
    },
    /// Permissionless, on-chain payout computation from PSPPool holder index.
    /// Processes one batch per call to stay within gas bounds.
    DisburseYield { limit: Option<u32> },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},
    #[returns(CycleStatusResponse)]
    CycleStatus {},
}

#[cw_serde]
pub struct ConfigResponse {
    pub admin: String,
    pub psp_pool: String,
    pub yield_reserve: String,
    pub investor_apy_bps: u64,
    pub cycle_days: u64,
    pub cycle_anchor_time: u64,
}

#[cw_serde]
pub struct CycleStatusResponse {
    pub completed_cycles: u64,
    pub last_disbursed_at: Option<u64>,
    pub next_cycle_time: u64,
    pub next_cycle_amount: Uint128,
    pub disbursement_in_progress: bool,
    pub next_start_after: Option<String>,
    pub distributed_amount: Uint128,
}

#[cw_serde]
pub struct MigrateMsg {}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::from_json;

    #[test]
    fn rejects_legacy_set_next_cycle_amount_execute_variant() {
        let legacy_payload = br#"{"set_next_cycle_amount":{"amount":"100"}}"#;
        let parsed = from_json::<ExecuteMsg>(legacy_payload);
        assert!(parsed.is_err());
    }

    #[test]
    fn accepts_current_disburse_yield_execute_variant() {
        let payload = br#"{"disburse_yield":{"limit":25}}"#;
        let parsed = from_json::<ExecuteMsg>(payload).unwrap();

        match parsed {
            ExecuteMsg::DisburseYield { limit } => assert_eq!(limit, Some(25)),
            _ => panic!("unexpected execute variant"),
        }
    }
}

```

## yield-distributor/src/state.rs

```
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::Item;

#[cw_serde]
pub struct Config {
    pub admin: Addr,
    pub psp_pool: Addr,
    pub yield_reserve: Addr,
    pub investor_apy_bps: u64,
    pub cycle_days: u64,
    pub cycle_anchor_time: u64,
}

#[cw_serde]
pub struct CycleState {
    pub completed_cycles: u64,
    pub last_disbursed_at: Option<u64>,
    pub active_disbursement: Option<ActiveDisbursement>,
}

#[cw_serde]
pub struct ActiveDisbursement {
    pub total_lp_supply: Uint128,
    pub cycle_amount: Uint128,
    pub distributed_amount: Uint128,
    pub next_start_after: Option<String>,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const CYCLE_STATE: Item<CycleState> = Item::new("cycle_state");

pub const CONTRACT_NAME: &str = "crates.io:defa-yield-distributor";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

```

## yield-reserve/src/contract.rs

```
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

```

## yield-reserve/src/error.rs

```
use cosmwasm_std::{StdError, Uint128};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
        #[error("LP obligations not settled; cannot claim protocol revenue")] 
        LpObligationsNotSettled {},
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized: caller={caller}, expected={expected}")]
    Unauthorized { caller: String, expected: String },

    #[error("Only CreditManager can call this endpoint")]
    CreditManagerOnly {},

    #[error("Only YieldDistributor can call this endpoint")]
    YieldDistributorOnly {},

    #[error("Invalid zero amount")]
    InvalidZeroAmount {},

    #[error("Expected exactly one {denom} coin in funds")]
    InvalidFunds { denom: String },

    #[error("No funds should be sent with this message")]
    UnexpectedFunds {},

    #[error("Accounting overflow while computing {operation}")]
    ArithmeticOverflow { operation: String },

    #[error("Insufficient accounted balance: available={available}, required={required}")]
    InsufficientAccountedBalance {
        available: Uint128,
        required: Uint128,
    },

    #[error("Insufficient contract balance: available={available}, required={required}")]
    InsufficientContractBalance {
        available: Uint128,
        required: Uint128,
    },

    #[error("No protocol revenue is currently claimable")]
    NoProtocolRevenue {},

    #[error("Claim amount exceeds claimable protocol revenue: claimable={claimable}, requested={requested}")]
    ClaimAmountExceedsClaimable {
        claimable: Uint128,
        requested: Uint128,
    },

    #[error("Accounting underflow: {context}")]
    AccountingUnderflow { context: String },
}

```

## yield-reserve/src/lib.rs

```
pub mod contract;
mod error;
pub mod msg;
pub mod state;

pub use crate::error::ContractError;

```

## yield-reserve/src/msg.rs

```
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;

#[cw_serde]
pub struct InstantiateMsg {
    pub admin: String,
    pub credit_manager: String,
    pub yield_distributor: String,
    pub stablecoin_denom: String,
    pub psp_pool: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    ReceiveFrom { amount: Uint128 },
    Disburse { recipient: String, amount: Uint128 },
    UpdateWiring {
        credit_manager: Option<String>,
        yield_distributor: Option<String>,
    },
    AdminTopUp { amount: Uint128 },
    ClaimProtocolRevenue { amount: Option<Uint128> },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},
    #[returns(BalanceResponse)]
    Balance {},
    #[returns(AccountingResponse)]
    Accounting {},
}

#[cw_serde]
pub struct ConfigResponse {
    pub admin: String,
    pub credit_manager: String,
    pub yield_distributor: String,
    pub stablecoin_denom: String,
}

#[cw_serde]
pub struct BalanceResponse {
    pub amount: Uint128,
    pub denom: String,
}

#[cw_serde]
pub struct AccountingResponse {
    pub fees_collected: Uint128,
    pub admin_topups: Uint128,
    pub lp_yield_disbursed: Uint128,
    pub protocol_revenue_claimed: Uint128,
    pub protocol_revenue_claimable: Uint128,
    pub accounted_available_balance: Uint128,
}

#[cw_serde]
pub struct MigrateMsg {}

```

## yield-reserve/src/state.rs

```
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::Item;

#[cw_serde]
pub struct Config {
    pub admin: Addr,
    pub credit_manager: Addr,
    pub yield_distributor: Addr,
    pub stablecoin_denom: String,
    pub psp_pool: Addr,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const FEES_COLLECTED: Item<Uint128> = Item::new("fees_collected");
pub const ADMIN_TOPUPS: Item<Uint128> = Item::new("admin_topups");
pub const LP_YIELD_DISBURSED: Item<Uint128> = Item::new("lp_yield_disbursed");
pub const PROTOCOL_REVENUE_CLAIMED: Item<Uint128> = Item::new("protocol_revenue_claimed");

pub const CONTRACT_NAME: &str = "crates.io:defa-yield-reserve";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

```

