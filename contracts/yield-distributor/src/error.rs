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
