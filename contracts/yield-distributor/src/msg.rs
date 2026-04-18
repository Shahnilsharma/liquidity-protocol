use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;

#[cw_serde]
pub struct InstantiateMsg {
    pub admin: String,
    pub psp_pool: String,
    pub yield_reserve: String,
    pub investor_apy_bps: u64,
    pub cycle_days: u64,
    pub cycle_anchor_time: Option<u64>,
}

#[cw_serde]
pub enum ExecuteMsg {
    UpdateWiring {
        psp_pool: Option<String>,
        yield_reserve: Option<String>,
    },
    /// Permissionless, on-chain payout computation from PSPPool holder index.
    /// Processes one batch per call to stay within gas bounds.
    DisburseYield { limit: Option<u32> },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},
    #[returns(CycleStatusResponse)]
    CycleStatus {},
}

#[cw_serde]
pub struct ConfigResponse {
    pub admin: String,
    pub psp_pool: String,
    pub yield_reserve: String,
    pub investor_apy_bps: u64,
    pub cycle_days: u64,
    pub cycle_anchor_time: u64,
}

#[cw_serde]
pub struct CycleStatusResponse {
    pub completed_cycles: u64,
    pub last_disbursed_at: Option<u64>,
    pub next_cycle_time: u64,
    pub next_cycle_amount: Uint128,
    pub disbursement_in_progress: bool,
    pub next_start_after: Option<String>,
    pub distributed_amount: Uint128,
}

#[cw_serde]
pub struct MigrateMsg {}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::from_json;

    #[test]
    fn rejects_legacy_set_next_cycle_amount_execute_variant() {
        let legacy_payload = br#"{"set_next_cycle_amount":{"amount":"100"}}"#;
        let parsed = from_json::<ExecuteMsg>(legacy_payload);
        assert!(parsed.is_err());
    }

    #[test]
    fn accepts_current_disburse_yield_execute_variant() {
        let payload = br#"{"disburse_yield":{"limit":25}}"#;
        let parsed = from_json::<ExecuteMsg>(payload).unwrap();

        match parsed {
            ExecuteMsg::DisburseYield { limit } => assert_eq!(limit, Some(25)),
            _ => panic!("unexpected execute variant"),
        }
    }
}
