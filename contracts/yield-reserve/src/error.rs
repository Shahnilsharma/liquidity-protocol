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
