
use cosmwasm_std::Addr;
use crate::contract::{execute, instantiate, query};
use crate::error::ContractError;
use crate::msg::{ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg, VaultInfoResponse};
use crate::state::PENDING_WITHDRAWALS;

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, message_info};
    use cosmwasm_std::{coins, from_json, Uint128};

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: Some(false),
            uri: None,
            uri_hash: None,
            description: Some("LP Token for Liquidity Pool".to_string()),
            admin: None,
            withdrawal_delay_seconds: 172_800, // 2 days
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(1, res.messages.len());

        let res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
        let config: ConfigResponse = from_json(&res).unwrap();
        assert_eq!("uzig", config.stablecoin_denom);
        assert!(config.lp_full_denom.contains("lplp"));
        assert_eq!(config.admin, Addr::unchecked("creator"));
        assert_eq!(172_800, config.withdrawal_delay);
    }

    #[test]
    fn test_invalid_subdenom() {
        let mut deps = mock_dependencies();

        // Too short
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "ab".to_string(),
            lp_minting_cap: Uint128::new(1_000_000),
            can_change_minting_cap: None,
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120, // 2 minutes (minimum)
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
        let err = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap_err();
        assert!(matches!(err, ContractError::InvalidSubdenom { .. }));

        // Doesn't start with lowercase
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "ABC".to_string(),
            lp_minting_cap: Uint128::new(1_000_000),
            can_change_minting_cap: None,
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120, // 2 minutes (minimum)
        };
        let err = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert!(matches!(err, ContractError::InvalidSubdenom { .. }));
    }

    #[test]
    fn test_zero_minting_cap() {
        let mut deps = mock_dependencies();
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::zero(),
            can_change_minting_cap: None,
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120, // 2 minutes (minimum)
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
        let err = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert!(matches!(err, ContractError::InvalidMintingCap {}));
    }

    #[test]
    fn test_withdrawal_delay_validation() {
        let mut deps = mock_dependencies();
        
        // Test too short delay (less than 2 minutes)
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000),
            can_change_minting_cap: None,
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 100, // Too short (< 120)
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
        let err = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap_err();
        assert!(matches!(err, ContractError::InvalidWithdrawalDelay { .. }));

        // Test too long delay
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000),
            can_change_minting_cap: None,
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 10_000_000, // Too long
        };
        let err = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap_err();
        assert!(matches!(err, ContractError::InvalidWithdrawalDelay { .. }));

        // Test valid minimum delay (2 minutes)
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000),
            can_change_minting_cap: None,
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120, // Exactly at minimum - valid
        };
        let res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        assert_eq!(1, res.messages.len());
        
        // Verify the delay was set
        let config_res = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
        let config: ConfigResponse = from_json(&config_res).unwrap();
        assert_eq!(120, config.withdrawal_delay);

        // Test valid custom delay (1 day)
        let mut deps2 = mock_dependencies();
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000),
            can_change_minting_cap: None,
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 86400, // 1 day - valid
        };
        let res = instantiate(deps2.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(1, res.messages.len());
        
        // Verify the delay was set
        let config_res = query(deps2.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
        let config: ConfigResponse = from_json(&config_res).unwrap();
        assert_eq!(86400, config.withdrawal_delay);
    }

    #[test]
    fn test_time_locked_withdrawal_flow() {
        let mut deps = mock_dependencies();
        let mut env = mock_env();

        // Initialize contract with custom delay for faster testing
        let custom_delay = 7200; // 2 hours
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: Some(false),
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: custom_delay,
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
        instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

        // Get the LP denom
        let config_res = query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap();
        let config: ConfigResponse = from_json(&config_res).unwrap();
        let lp_denom = config.lp_full_denom;
        assert_eq!(custom_delay, config.withdrawal_delay);

        // Test deposit (simulated - in real chain TokenFactory would mint)
        let info = message_info(&Addr::unchecked("user"), &coins(1000, "uzig"));
        let res = execute(deps.as_mut(), env.clone(), info, ExecuteMsg::Deposit {}).unwrap();
        assert_eq!(1, res.messages.len());

        // Test request withdrawal
        let info = message_info(&Addr::unchecked("user"), &coins(500, &lp_denom));
        let res = execute(
            deps.as_mut(),
            env.clone(),
            info,
            ExecuteMsg::RequestWithdraw {},
        )
        .unwrap();
        assert_eq!(1, res.messages.len());

        // Check vault state includes pending withdrawals
        let vault_info_res = query(deps.as_ref(), env.clone(), QueryMsg::VaultInfo {}).unwrap();
        let vault_info: VaultInfoResponse = from_json(&vault_info_res).unwrap();
        assert_eq!(Uint128::new(500), vault_info.total_pending_withdrawals);

        // Query pending withdrawals directly from storage
        let user_addr = Addr::unchecked("user");
        let pending_withdrawal = PENDING_WITHDRAWALS
            .load(deps.as_ref().storage, (&user_addr, 0))
            .unwrap();
        assert_eq!(Uint128::new(500), pending_withdrawal.amount);
        assert!(env.block.time < pending_withdrawal.release_time);

        // Try to claim too early
        let info = message_info(&Addr::unchecked("user"), &[]);
        let err = execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ClaimWithdraw { withdrawal_id: 0 },
        )
        .unwrap_err();
        assert!(matches!(err, ContractError::WithdrawalLocked { .. }));

        // Advance time by custom delay
        env.block.time = env.block.time.plus_seconds(custom_delay);

        // Try to claim - will fail because contract has no stablecoin balance
        // In production, admin would ensure liquidity by keeping funds in contract
        // or bringing them back from yield protocols before users claim
        let err = execute(
            deps.as_mut(),
            env.clone(),
            info.clone(),
            ExecuteMsg::ClaimWithdraw { withdrawal_id: 0 },
        )
        .unwrap_err();
        assert!(matches!(
            err,
            ContractError::InsufficientContractBalance { .. }
        ));

        // Verify withdrawal still exists (not claimed yet)
        let withdrawal_exists = PENDING_WITHDRAWALS
            .may_load(deps.as_ref().storage, (&user_addr, 0))
            .unwrap();
        assert!(withdrawal_exists.is_some());

        // Verify vault state still shows pending withdrawal
        let vault_info_res = query(deps.as_ref(), env.clone(), QueryMsg::VaultInfo {}).unwrap();
        let vault_info: VaultInfoResponse = from_json(&vault_info_res).unwrap();
        assert_eq!(Uint128::new(500), vault_info.total_pending_withdrawals);
        // Total deposited stays at 1000 because withdrawal hasn't been paid out yet
        assert_eq!(Uint128::new(1000), vault_info.total_deposited);
    }

    #[test]
    fn test_adversarial_rounding_deposit() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: None,
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120,
        };
        instantiate(deps.as_mut(), env.clone(), message_info(&Addr::unchecked("creator"), &[]), msg).unwrap();

        // 1. Initial deposit of 1 unit to set a baseline
        execute(deps.as_mut(), env.clone(), message_info(&Addr::unchecked("user1"), &coins(1, "uzig")), ExecuteMsg::Deposit {}).unwrap();

        // 2. Admin adds yield to inflate share price (1 uzig -> 1000 uzig value)
        // Price = 1000 / 1 = 1000 uzig per share
        execute(deps.as_mut(), env.clone(), message_info(&Addr::unchecked("creator"), &coins(999, "uzig")), ExecuteMsg::AdminDepositYield {
            principal_amount: Uint128::zero(),
            yield_amount: Uint128::new(999),
        }).unwrap();

        // 3. Rounding Attack: User deposits 999 uzig. 
        // Shares = amount / price = 999 / 1000 = 0.999 -> rounded down to 0
        let user2_info = message_info(&Addr::unchecked("user2"), &coins(999, "uzig"));
        let err = execute(deps.as_mut(), env.clone(), user2_info, ExecuteMsg::Deposit {}).unwrap_err();
        
        // Contract should reject zero-share deposits to prevent loss of funds
        // In this implementation, it returns a StdError with a specific message
        match err {
            ContractError::Std(e) => assert!(e.to_string().contains("Deposit too small")),
            _ => panic!("Expected StdError with 'Deposit too small', got {:?}", err),
        }
    }

    #[test]
    fn test_admin_yield_negative_scenario() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: None,
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120,
        };
        instantiate(deps.as_mut(), env.clone(), message_info(&Addr::unchecked("creator"), &[]), msg).unwrap();

        // 1. Initial deposit of 1000 units
        execute(deps.as_mut(), env.clone(), message_info(&Addr::unchecked("user1"), &coins(1000, "uzig")), ExecuteMsg::Deposit {}).unwrap();

        // Admin returns principal (previously withdrawn) but adds 500 yield
        let admin_info = message_info(&Addr::unchecked("creator"), &coins(1500, "uzig"));
        execute(
            deps.as_mut(),
            env.clone(),
            admin_info,
            ExecuteMsg::AdminDepositYield {
                principal_amount: Uint128::new(1000),
                yield_amount: Uint128::new(500),
            },
        ).unwrap();

        let vault_info: VaultInfoResponse = from_json(&query(deps.as_ref(), env.clone(), QueryMsg::VaultInfo {}).unwrap()).unwrap();
        // Total deposited should be 1000 (orig) + 500 (yield) = 1500
        assert_eq!(Uint128::new(1500), vault_info.total_deposited);
        assert_eq!("1.500000", vault_info.price_per_share);
    }

    #[test]
    fn test_admin_deposit_yield_success() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        // Initialize contract (admin defaults to creator)
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: Some(false),
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120,
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
        instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

        // User deposits 1000 uzig
        let user_info = message_info(&Addr::unchecked("user"), &coins(1000, "uzig"));
        execute(deps.as_mut(), env.clone(), user_info, ExecuteMsg::Deposit {}).unwrap();

        // Check initial vault state
        let vault_info = query(deps.as_ref(), env.clone(), QueryMsg::VaultInfo {}).unwrap();
        let vault: VaultInfoResponse = from_json(&vault_info).unwrap();
        assert_eq!(Uint128::new(1000), vault.total_deposited);
        assert_eq!(Uint128::new(1000), vault.total_lp_supply);
        assert_eq!("1.000000", vault.price_per_share); // Initial 1:1

        // Admin (creator) deposits 100 uzig as pure yield (no principal return)
        let admin_info = message_info(&Addr::unchecked("creator"), &coins(100, "uzig"));
        let res = execute(
            deps.as_mut(),
            env.clone(),
            admin_info,
            ExecuteMsg::AdminDepositYield {
                principal_amount: Uint128::zero(),
                yield_amount: Uint128::new(100),
            },
        )
        .unwrap();

        // Verify response attributes
        assert_eq!("admin_deposit_yield", res.attributes[0].value);
        assert_eq!("0", res.attributes[2].value); // principal_returned
        assert_eq!("100", res.attributes[3].value); // yield_deposited
        assert_eq!("100", res.attributes[4].value); // total_received
        assert_eq!("1000", res.attributes[5].value); // old_total_deposited
        assert_eq!("1100", res.attributes[6].value); // new_total_deposited
        assert_eq!("1.100000", res.attributes[7].value); // price_per_share

        // Verify vault state updated correctly
        let vault_info = query(deps.as_ref(), env.clone(), QueryMsg::VaultInfo {}).unwrap();
        let vault: VaultInfoResponse = from_json(&vault_info).unwrap();
        assert_eq!(Uint128::new(1100), vault.total_deposited); // Increased by yield
        assert_eq!(Uint128::new(1000), vault.total_lp_supply); // LP supply unchanged
        assert_eq!("1.100000", vault.price_per_share); // 10% increase
    }

    #[test]
    fn test_admin_deposit_yield_unauthorized() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        // Initialize (admin defaults to creator)
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: Some(false),
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120,
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
        instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

        // Try to deposit yield as non-admin
        let non_admin_info = message_info(&Addr::unchecked("attacker"), &coins(100, "uzig"));
        let err = execute(
            deps.as_mut(),
            env,
            non_admin_info,
            ExecuteMsg::AdminDepositYield {
                principal_amount: Uint128::zero(),
                yield_amount: Uint128::new(100),
            },
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::Unauthorized {}));
    }

    #[test]
    fn test_admin_deposit_yield_wrong_denom() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        // Initialize (admin defaults to creator)
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: Some(false),
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120,
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
        instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

        // Try to deposit wrong token as admin (creator)
        let admin_info = message_info(&Addr::unchecked("creator"), &coins(100, "wrongtoken"));
        let err = execute(
            deps.as_mut(),
            env,
            admin_info,
            ExecuteMsg::AdminDepositYield {
                principal_amount: Uint128::zero(),
                yield_amount: Uint128::new(100),
            },
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::NoStablecoinSent {}));
    }

    #[test]
    fn test_admin_deposit_yield_zero_amount() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        // Initialize (admin defaults to creator)
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: Some(false),
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120,
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
        instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

        // Try to deposit zero amount as admin (creator)
        let admin_info = message_info(&Addr::unchecked("creator"), &coins(0, "uzig"));
        let err = execute(
            deps.as_mut(),
            env,
            admin_info,
            ExecuteMsg::AdminDepositYield {
                principal_amount: Uint128::zero(),
                yield_amount: Uint128::zero(),
            },
        )
        .unwrap_err();

        assert!(matches!(err, ContractError::InvalidZeroAmount {}));
    }

    #[test]
    fn test_admin_deposit_yield_multiple_deposits() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        // Initialize (admin defaults to creator)
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: Some(false),
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120,
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
        instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

        // User deposits 1000 uzig
        let user_info = message_info(&Addr::unchecked("user"), &coins(1000, "uzig"));
        execute(deps.as_mut(), env.clone(), user_info, ExecuteMsg::Deposit {}).unwrap();

        // Admin (creator) deposits yield multiple times
        let admin_info = message_info(&Addr::unchecked("creator"), &coins(50, "uzig"));
        execute(
            deps.as_mut(),
            env.clone(),
            admin_info.clone(),
            ExecuteMsg::AdminDepositYield {
                principal_amount: Uint128::zero(),
                yield_amount: Uint128::new(50),
            },
        )
        .unwrap();

        // Check after first deposit: 1000 + 50 = 1050
        let vault_info = query(deps.as_ref(), env.clone(), QueryMsg::VaultInfo {}).unwrap();
        let vault: VaultInfoResponse = from_json(&vault_info).unwrap();
        assert_eq!(Uint128::new(1050), vault.total_deposited);
        assert_eq!("1.050000", vault.price_per_share);

        // Second yield deposit by admin (creator)
        let admin_info2 = message_info(&Addr::unchecked("creator"), &coins(100, "uzig"));
        execute(
            deps.as_mut(),
            env.clone(),
            admin_info2,
            ExecuteMsg::AdminDepositYield {
                principal_amount: Uint128::zero(),
                yield_amount: Uint128::new(100),
            },
        )
        .unwrap();

        // Check after second deposit: 1050 + 100 = 1150
        let vault_info = query(deps.as_ref(), env.clone(), QueryMsg::VaultInfo {}).unwrap();
        let vault: VaultInfoResponse = from_json(&vault_info).unwrap();
        assert_eq!(Uint128::new(1150), vault.total_deposited);
        assert_eq!("1.150000", vault.price_per_share); // 15% total yield

        // Verify LP supply unchanged
        assert_eq!(Uint128::new(1000), vault.total_lp_supply);
    }

    #[test]
    fn test_admin_deposit_yield_no_lp_holders() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        // Initialize (admin defaults to creator)
        let msg = InstantiateMsg {
            stablecoin_denom: "uzig".to_string(),
            lp_subdenom: "lplp".to_string(),
            lp_minting_cap: Uint128::new(1_000_000_000),
            can_change_minting_cap: Some(false),
            uri: None,
            uri_hash: None,
            description: None,
            admin: None,
            withdrawal_delay_seconds: 120,
        };
        let info = message_info(&Addr::unchecked("creator"), &[]);
        instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

        // Admin (creator) tries to deposit yield when no users have deposited yet
        let admin_info = message_info(&Addr::unchecked("creator"), &coins(100, "uzig"));
        let res = execute(
            deps.as_mut(),
            env.clone(),
            admin_info,
            ExecuteMsg::AdminDepositYield {
                principal_amount: Uint128::zero(),
                yield_amount: Uint128::new(100),
            },
        )
        .unwrap();

        // Should succeed - total_deposited increases
        assert_eq!("admin_deposit_yield", res.attributes[0].value);
        assert_eq!("0", res.attributes[2].value); // principal_returned
        assert_eq!("100", res.attributes[3].value); // yield_deposited

        // Verify vault state
        let vault_info = query(deps.as_ref(), env, QueryMsg::VaultInfo {}).unwrap();
        let vault: VaultInfoResponse = from_json(&vault_info).unwrap();
        assert_eq!(Uint128::new(100), vault.total_deposited);
        assert_eq!(Uint128::new(0), vault.total_lp_supply); // No LP tokens
        assert_eq!("1.0", vault.price_per_share); // Default when no LP
    }
}
