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
        /// User address
        address: String,
        /// Withdrawal ID
        withdrawal_id: u64,
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
}

/// Response for VaultInfo query
#[cw_serde]
pub struct VaultInfoResponse {
    /// Total stablecoins deposited in the vault
    pub total_stablecoin_deposited: Uint128,
    /// Total LP tokens in circulation
    pub total_lp_supply: Uint128,
    /// Total amount locked in pending withdrawals
    pub total_pending_withdrawals: Uint128,
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
}

/// Information about a single pending withdrawal
#[cw_serde]
pub struct WithdrawalInfo {
    /// Withdrawal ID
    pub id: u64,
    /// Amount of stablecoins to be withdrawn
    pub amount: Uint128,
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

/// Migration message (for future upgrades)
#[cw_serde]
pub struct MigrateMsg {}
