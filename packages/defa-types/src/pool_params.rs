use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Uint128};

#[cw_serde]
pub struct PoolInitParams {
    pub hard_cap: Uint128,
    pub soft_cap: Uint128,
    pub facility_tenure_days: u64,
    pub psp_rate_bps_per_day: u64,
    pub investor_apy_bps: u64,
    pub drawdown_limit: Uint128,
    pub drawdown_tenor_days: u64,
    pub penalty_rate_bps_per_day: u64,
    pub withdrawal_window_day: u8,
    pub yield_cycle_days: u64,
    pub max_queue_wait_days: Option<u64>,
    pub psp_address: Addr,
    pub psp_identifier: String,
    pub credit_manager: Addr,
    pub yield_distributor: Addr,
    pub yield_reserve: Addr,
}
