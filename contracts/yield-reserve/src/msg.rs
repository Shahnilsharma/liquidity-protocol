use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;

#[cw_serde]
pub struct InstantiateMsg {
    pub admin: String,
    pub credit_manager: String,
    pub yield_distributor: String,
    pub stablecoin_denom: String,
    pub psp_pool: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    ReceiveFrom { amount: Uint128 },
    Disburse { recipient: String, amount: Uint128 },
    UpdateWiring {
        credit_manager: Option<String>,
        yield_distributor: Option<String>,
    },
    AdminTopUp { amount: Uint128 },
    ClaimProtocolRevenue { amount: Option<Uint128> },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},
    #[returns(BalanceResponse)]
    Balance {},
    #[returns(AccountingResponse)]
    Accounting {},
}

#[cw_serde]
pub struct ConfigResponse {
    pub admin: String,
    pub credit_manager: String,
    pub yield_distributor: String,
    pub stablecoin_denom: String,
}

#[cw_serde]
pub struct BalanceResponse {
    pub amount: Uint128,
    pub denom: String,
}

#[cw_serde]
pub struct AccountingResponse {
    pub fees_collected: Uint128,
    pub admin_topups: Uint128,
    pub lp_yield_disbursed: Uint128,
    pub protocol_revenue_claimed: Uint128,
    pub protocol_revenue_claimable: Uint128,
    pub accounted_available_balance: Uint128,
}

#[cw_serde]
pub struct MigrateMsg {}
