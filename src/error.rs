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
}
