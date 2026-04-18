use cosmwasm_schema::cw_serde;

#[cw_serde]
pub enum PoolState {
    Fundraising,
    Active,
    WindingDown,
    Settled,
    Closed,
}
