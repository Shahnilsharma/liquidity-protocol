use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use cw_storage_plus::Item;

/// Contract configuration stored in state
#[cw_serde]
pub struct Config {
    /// Address of the CW20 stablecoin token (e.g., USDT)
    pub stablecoin_address: Addr,
    /// Address of the CW20 LP token contract
    pub lp_token_address: Addr,
    /// Admin address that can update config
    pub admin: Addr,
}

/// Pool statistics
#[cw_serde]
pub struct PoolState {
    /// Total amount of stablecoins deposited in the pool
    pub total_stablecoin_deposited: cosmwasm_std::Uint128,
    /// Total LP tokens minted (should match CW20 supply)
    pub total_lp_minted: cosmwasm_std::Uint128,
}

// Storage keys
pub const CONFIG: Item<Config> = Item::new("config");
pub const POOL_STATE: Item<PoolState> = Item::new("pool_state");

// Contract name and version for migration support
pub const CONTRACT_NAME: &str = "crates.io:lp-pool-contract";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
