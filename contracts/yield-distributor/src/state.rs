use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};
use cw_storage_plus::Item;

#[cw_serde]
pub struct Config {
    pub admin: Addr,
    pub psp_pool: Addr,
    pub yield_reserve: Addr,
    pub investor_apy_bps: u64,
    pub cycle_days: u64,
    pub cycle_anchor_time: u64,
}

#[cw_serde]
pub struct CycleState {
    pub completed_cycles: u64,
    pub last_disbursed_at: Option<u64>,
    pub active_disbursement: Option<ActiveDisbursement>,
}

#[cw_serde]
pub struct ActiveDisbursement {
    pub total_lp_supply: Uint128,
    pub cycle_amount: Uint128,
    pub distributed_amount: Uint128,
    pub next_start_after: Option<String>,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const CYCLE_STATE: Item<CycleState> = Item::new("cycle_state");

pub const CONTRACT_NAME: &str = "crates.io:defa-yield-distributor";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
