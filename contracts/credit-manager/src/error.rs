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
