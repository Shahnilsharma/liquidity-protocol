use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use cw_storage_plus::Item;

/// Contract configuration stored in state
#[cw_serde]
pub struct Config {
    /// Denom of the stablecoin (e.g., "uzig" or TokenFactory denom)
    pub stablecoin_denom: String,
    /// Full denom of the LP token created by this contract
    /// Format: coin.{contract_address}.{subdenom}
    pub lp_full_denom: String,
    /// Admin address that can update config
    pub admin: Addr,
}

/// Pool statistics
#[cw_serde]
pub struct PoolState {
    /// Total amount of stablecoins deposited in the pool
    pub total_stablecoin_deposited: cosmwasm_std::Uint128,
    /// Total LP tokens minted
    pub total_lp_minted: cosmwasm_std::Uint128,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const POOL_STATE: Item<PoolState> = Item::new("pool_state");

pub const CONTRACT_NAME: &str = "crates.io:lp-pool-tokenfactory";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
