use cosmwasm_schema::cw_serde;
use cosmwasm_std::Uint128;

#[cw_serde]
pub enum DrawdownStatus {
    Active,
    Overdue,
    Settled,
}

#[cw_serde]
pub struct Drawdown {
    pub id: u64,
    pub principal: Uint128,
    pub timestamp: u64,
    pub due_date: u64,
    pub status: DrawdownStatus,
    pub settled_at: Option<u64>,
    pub fee_paid: Option<Uint128>,
}
