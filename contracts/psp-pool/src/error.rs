
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
