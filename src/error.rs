use cosmwasm_std::StdError;
use thiserror::Error;

/// Custom errors for the LP pool contract
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

    #[error("Insufficient pool balance - pool does not have enough stablecoins")]
    InsufficientPoolBalance {},

    #[error("Invalid address - {0}")]
    InvalidAddress(String),

    #[error("Transfer failed - {0}")]
    TransferFailed(String),

    #[error("Mint failed - {0}")]
    MintFailed(String),

    #[error("Burn failed - {0}")]
    BurnFailed(String),

    #[error("Query failed - {0}")]
    QueryFailed(String),

    #[error("Pool state inconsistent - total deposited does not match LP supply")]
    InconsistentPoolState {},
}
