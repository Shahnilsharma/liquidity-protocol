use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Binary};
use cw_storage_plus::{Item, Map};

#[cw_serde]
pub struct Config {
    pub admin: Addr,
    pub psp_pool_code_id: u64,
    pub credit_manager_code_id: u64,
    pub yield_distributor_code_id: u64,
    pub yield_reserve_code_id: u64,
}

#[cw_serde]
pub struct PoolRecord {
    pub pool_id: u64,
    pub psp_pool: Addr,
    pub credit_manager: Addr,
    pub yield_distributor: Addr,
    pub yield_reserve: Addr,
    pub created_at: u64,
}

#[cw_serde]
pub struct PendingPoolCreation {
    pub pool_id: u64,
    pub label_prefix: String,
    pub created_at: u64,
    pub psp_pool_init_msg: Binary,
    pub credit_manager_init_msg: Binary,
    pub yield_distributor_init_msg: Binary,
    pub yield_reserve_init_msg: Binary,
    pub psp_pool: Option<Addr>,
    pub credit_manager: Option<Addr>,
    pub yield_distributor: Option<Addr>,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const NEXT_POOL_ID: Item<u64> = Item::new("next_pool_id");
pub const POOLS: Map<u64, PoolRecord> = Map::new("pools");
pub const PENDING_POOL_CREATION: Item<PendingPoolCreation> = Item::new("pending_pool_creation");

pub const CONTRACT_NAME: &str = "crates.io:defa-pool-factory";
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const REPLY_ID_PSP_POOL: u64 = 1;
pub const REPLY_ID_CREDIT_MANAGER: u64 = 2;
pub const REPLY_ID_YIELD_DISTRIBUTOR: u64 = 3;
pub const REPLY_ID_YIELD_RESERVE: u64 = 4;
