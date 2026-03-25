// Minimal reproducer for adversarial action sequence failure
mod fuzz_helpers;
mod reference_model;

use fuzz_helpers::*;
use reference_model::ReferenceVault;

#[test]
fn debug_failing_sequence() {
    let mut vault = setup_vault(120);
    let mut reference = ReferenceVault::new();
    
    println!("=== Initial State ===");
    println!("Vault: deposited={}, shares={}", vault.total_deposited, vault.total_lp_supply);
    println!("Reference: assets={}, shares={}", reference.total_assets, reference.total_shares);

    // Action 1: AdminReturnZero
    println!("\n=== Action 1: AdminReturnZero ===");
    let vault_result = execute_admin_deposit_yield(&mut vault, 0, 0);
    let ref_result = reference.admin_deposit_yield(0, 0);
    println!("Vault result: {:?}", vault_result);
    println!("Reference result: {:?}", ref_result);
    println!("Vault: deposited={}, shares={}", vault.total_deposited, vault.total_lp_supply);
    println!("Reference: assets={}, shares={}", reference.total_assets, reference.total_shares);

    // Action 2: AdminReturnZero
    println!("\n=== Action 2: AdminReturnZero ===");
    let vault_result = execute_admin_deposit_yield(&mut vault, 0, 0);
    let ref_result = reference.admin_deposit_yield(0, 0);
    println!("Vault result: {:?}", vault_result);
    println!("Reference result: {:?}", ref_result);
    println!("Vault: deposited={}, shares={}", vault.total_deposited, vault.total_lp_supply);
    println!("Reference: assets={}, shares={}", reference.total_assets, reference.total_shares);

    // Action 3: Deposit
    println!("\n=== Action 3: Deposit user_c 7058723 ===");
    let vault_result = execute_deposit(&mut vault, "user_c", 7058723);
    let ref_result = reference.deposit("user_c", 7058723);
    println!("Vault result: {:?}", vault_result);
    println!("Reference result: {:?}", ref_result);
    println!("Vault: deposited={}, shares={}", vault.total_deposited, vault.total_lp_supply);
    println!("Reference: assets={}, shares={}", reference.total_assets, reference.total_shares);

    // Action 4: AdminReturnZero
    println!("\n=== Action 4: AdminReturnZero ===");
    let vault_result = execute_admin_deposit_yield(&mut vault, 0, 0);
    let ref_result = reference.admin_deposit_yield(0, 0);
    println!("Vault result: {:?}", vault_result);
    println!("Reference result: {:?}", ref_result);
    println!("Vault: deposited={}, shares={}", vault.total_deposited, vault.total_lp_supply);
    println!("Reference: assets={}, shares={}", reference.total_assets, reference.total_shares);

    // Action 5: AdminWithdraw
    println!("\n=== Action 5: AdminWithdraw 1000000 ===");
    let vault_result = execute_admin_withdraw(&mut vault, 1000000);
    let ref_result = reference.admin_withdraw(1000000);
    println!("Vault result: {:?}", vault_result);
    println!("Reference result: {:?}", ref_result);
    println!("Vault: deposited={}, shares={}", vault.total_deposited, vault.total_lp_supply);
    println!("Reference: assets={}, shares={}", reference.total_assets, reference.total_shares);

    // Action 6: WithdrawAll
    println!("\n=== Action 6: WithdrawAll user_c ===");
    let shares = query_lp_balance(&vault, "user_c");
    println!("User shares: {}", shares);
    if shares > 0 {
        let vault_result = execute_withdraw(&mut vault, "user_c", shares);
        let ref_result = reference.withdraw("user_c", shares);
        println!("Vault result: {:?}", vault_result);
        println!("Reference result: {:?}", ref_result);
    }
    println!("Vault: deposited={}, shares={}", vault.total_deposited, vault.total_lp_supply);
    println!("Reference: assets={}, shares={}", reference.total_assets, reference.total_shares);

    // Final check
    println!("\n=== Final State ===");
    println!("Vault total_deposited: {}", vault.total_deposited);
    println!("Reference total_assets: {}", reference.total_assets);
    assert_eq!(vault.total_deposited, reference.total_assets, "State diverged!");
}
