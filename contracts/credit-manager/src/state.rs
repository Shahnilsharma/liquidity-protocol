use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::{Item, Map};
use defa_types::Drawdown;

#[cw_serde]
pub struct Config {
    pub admin: Addr,
    pub psp_address: Addr,
    pub psp_pool: Addr,
    pub yield_reserve: Addr,
    pub stablecoin_denom: String,
    pub drawdown_limit: Uint128,
    pub drawdown_tenor_days: u64,
    pub psp_rate_bps_per_day: u64,
    pub penalty_rate_bps_per_day: u64,
    pub max_queue_wait_days: Option<u64>,
}

#[cw_serde]
pub struct DiscretionaryWindow {
    pub start_time: u64,
    pub end_time: u64,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const DRAWDOWNS: Map<u64, Drawdown> = Map::new("drawdowns");
pub const NEXT_DRAWDOWN_ID: Item<u64> = Item::new("next_drawdown_id");
pub const OUTSTANDING_PRINCIPAL: Item<Uint128> = Item::new("outstanding_principal");
pub const OVERDUE_COUNT: Item<u64> = Item::new("overdue_count");
pub const BLOCKED: Item<bool> = Item::new("blocked");
pub const DISCRETIONARY_WINDOW: Item<DiscretionaryWindow> = Item::new("discretionary_window");

pub const CONTRACT_NAME: &str = "crates.io:defa-credit-manager";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
