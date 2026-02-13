use cosmwasm_std::StdError;
use thiserror::Error;

/// Custom errors for the token vault contract
#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized - only admin can perform this action")]
    Unauthorized {},

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

    #[error("No pending withdrawals found for user")]
    NoPendingWithdrawals {},

    #[error("Invalid withdrawal delay - must be between {min} and {max} seconds")]
    InvalidWithdrawalDelay { min: u64, max: u64 },

    #[error("Yield contract error - {reason}")]
    YieldContractError { reason: String },

    #[error("Invalid yield contract response - {reason}")]
    InvalidYieldResponse { reason: String },

    #[error("Insufficient liquidity in yield contract - cannot withdraw requested amount")]
    InsufficientYieldLiquidity {},

    #[error("Zero shares received from yield contract")]
    ZeroSharesReceived {},
}
