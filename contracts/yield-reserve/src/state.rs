use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::Item;

#[cw_serde]
pub struct Config {
    pub admin: Addr,
    pub credit_manager: Addr,
    pub yield_distributor: Addr,
    pub stablecoin_denom: String,
    pub psp_pool: Addr,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const FEES_COLLECTED: Item<Uint128> = Item::new("fees_collected");
pub const ADMIN_TOPUPS: Item<Uint128> = Item::new("admin_topups");
pub const LP_YIELD_DISBURSED: Item<Uint128> = Item::new("lp_yield_disbursed");
pub const PROTOCOL_REVENUE_CLAIMED: Item<Uint128> = Item::new("protocol_revenue_claimed");

pub const CONTRACT_NAME: &str = "crates.io:defa-yield-reserve";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
