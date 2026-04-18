
use cosmwasm_schema::{cw_serde, QueryResponses};
use defa_psp_pool;
use defa_credit_manager;
use defa_yield_distributor;
use defa_yield_reserve;

#[cw_serde]
pub struct InstantiateMsg {
    pub admin: String,
    pub psp_pool_code_id: u64,
    pub credit_manager_code_id: u64,
    pub yield_distributor_code_id: u64,
    pub yield_reserve_code_id: u64,
}

#[cw_serde]
pub enum ExecuteMsg {
    SetCodeIds {
        psp_pool_code_id: u64,
        credit_manager_code_id: u64,
        yield_distributor_code_id: u64,
        yield_reserve_code_id: u64,
    },
    CreatePool {
        label_prefix: String,
        psp_pool_init_msg: defa_psp_pool::msg::InstantiateMsg,
        credit_manager_init_msg: defa_credit_manager::msg::InstantiateMsg,
        yield_distributor_init_msg: defa_yield_distributor::msg::InstantiateMsg,
        yield_reserve_init_msg: defa_yield_reserve::msg::InstantiateMsg,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},
    #[returns(PoolResponse)]
    Pool { pool_id: u64 },
    #[returns(PoolsResponse)]
    Pools { start_after: Option<u64>, limit: Option<u32> },
}

#[cw_serde]
pub struct ConfigResponse {
    pub admin: String,
    pub psp_pool_code_id: u64,
    pub credit_manager_code_id: u64,
    pub yield_distributor_code_id: u64,
    pub yield_reserve_code_id: u64,
}

#[cw_serde]
pub struct PoolRecordResponse {
    pub pool_id: u64,
    pub psp_pool: String,
    pub credit_manager: String,
    pub yield_distributor: String,
    pub yield_reserve: String,
    pub created_at: u64,
}

#[cw_serde]
pub struct PoolResponse {
    pub pool: Option<PoolRecordResponse>,
}

#[cw_serde]
pub struct PoolsResponse {
    pub pools: Vec<PoolRecordResponse>,
}

#[cw_serde]
pub struct MigrateMsg {}
