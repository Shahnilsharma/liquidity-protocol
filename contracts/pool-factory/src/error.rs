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
