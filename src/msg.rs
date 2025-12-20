use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};

/// Message sent when instantiating the contract
#[cw_serde]
pub struct InstantiateMsg {
    /// Address of the CW20 stablecoin (USDT) contract
    pub stablecoin_address: String,
    /// Address of the CW20 LP token contract
    pub lp_token_address: String,
    /// Optional admin address (defaults to sender if not provided)
    pub admin: Option<String>,
}

/// Messages that can be executed on the contract
#[cw_serde]
pub enum ExecuteMsg {
    /// Deposit stablecoins to receive LP tokens
    /// User must have approved the contract beforehand
    Deposit {
        /// Amount of stablecoins to deposit
        amount: Uint128,
    },
    /// Withdraw stablecoins by burning LP tokens
    Withdraw {
        /// Amount of LP tokens to burn
        amount: Uint128,
    },
    /// Update contract configuration (admin only)
    UpdateConfig {
        /// New stablecoin address (optional)
        stablecoin_address: Option<String>,
        /// New LP token address (optional)
        lp_token_address: Option<String>,
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
    
    /// Get pool information (total deposits, LP supply, etc.)
    #[returns(PoolInfoResponse)]
    PoolInfo {},
    
    /// Get user's deposit information
    #[returns(UserInfoResponse)]
    UserInfo {
        /// User address to query
        address: String,
    },
}

/// Response for Config query
#[cw_serde]
pub struct ConfigResponse {
    /// Address of the stablecoin CW20 token
    pub stablecoin_address: Addr,
    /// Address of the LP token CW20 contract
    pub lp_token_address: Addr,
    /// Admin address
    pub admin: Addr,
}

/// Response for PoolInfo query
#[cw_serde]
pub struct PoolInfoResponse {
    /// Total stablecoins deposited in the pool
    pub total_stablecoin_deposited: Uint128,
    /// Total LP tokens in circulation
    pub total_lp_supply: Uint128,
    /// Exchange rate (stablecoin per LP token)
    pub exchange_rate: String,
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

/// Migration message (for future upgrades)
#[cw_serde]
pub struct MigrateMsg {}
