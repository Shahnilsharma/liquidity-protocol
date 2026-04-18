use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;
use defa_types::Drawdown;

#[cw_serde]
pub struct InstantiateMsg {
    pub admin: String,
    pub psp_address: String,
    pub psp_pool: String,
    pub yield_reserve: String,
    pub stablecoin_denom: String,
    pub drawdown_limit: Uint128,
    pub drawdown_tenor_days: u64,
    pub psp_rate_bps_per_day: u64,
    pub penalty_rate_bps_per_day: u64,
    pub max_queue_wait_days: Option<u64>,
}

#[cw_serde]
pub enum ExecuteMsg {
    RequestDrawdown { amount: Uint128 },
    Repay { drawdown_id: u64 },
    OverrideBlock { drawdown_id: u64, reason_code: String },
    SetDiscretionaryWindow { start_time: u64, end_time: u64 },
    UpdateWiring {
        psp_pool: Option<String>,
        yield_reserve: Option<String>,
        max_queue_wait_days: Option<u64>,
    },
    UpdatePspAddress { new_psp_address: String },
    SettleUnutilizedFee {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},
    #[returns(DrawdownResponse)]
    Drawdown { drawdown_id: u64 },
    #[returns(DrawdownsResponse)]
    Drawdowns {
        start_after: Option<u64>,
        limit: Option<u32>,
    },
    #[returns(SystemStatusResponse)]
    SystemStatus {},
    #[returns(FeePreviewResponse)]
    FeePreview {
        principal: Uint128,
        drawdown_timestamp: u64,
        current_timestamp: u64,
    },
}

#[cw_serde]
pub struct ConfigResponse {
    pub admin: String,
    pub psp_address: String,
    pub psp_pool: String,
    pub yield_reserve: String,
    pub stablecoin_denom: String,
    pub drawdown_limit: Uint128,
    pub drawdown_tenor_days: u64,
    pub psp_rate_bps_per_day: u64,
    pub penalty_rate_bps_per_day: u64,
    pub max_queue_wait_days: Option<u64>,
}

#[cw_serde]
pub struct DrawdownResponse {
    pub drawdown: Drawdown,
}

#[cw_serde]
pub struct DrawdownsResponse {
    pub drawdowns: Vec<Drawdown>,
}

#[cw_serde]
pub struct SystemStatusResponse {
    pub blocked: bool,
    pub outstanding_principal: Uint128,
    pub overdue_count: u64,
    pub next_drawdown_id: u64,
    pub discretionary_window_end: Option<u64>,
}

#[cw_serde]
pub struct FeePreviewResponse {
    pub fee: Uint128,
}

#[cw_serde]
pub struct MigrateMsg {}
