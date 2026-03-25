// Debug test for the new failing sequence
mod fuzz_helpers;
mod reference_model;

use fuzz_helpers::*;
use reference_model::ReferenceVault;

#[test]
fn debug_new_failing_sequence() {
    let mut vault = setup_vault(120);
    let mut reference = ReferenceVault::new();
    
    println!("=== Initial ===");
    println!("Vault: deposited={}, shares={}", vault.total_deposited, vault.total_lp_supply);
    println!("Reference: assets={}, shares={}", reference.total_assets, reference.total_shares);

    // Action 1: Deposit attacker 7767033
    println!("\n=== Action 1: Deposit attacker 7767033 ===");
    let v_shares = execute_deposit(&mut vault, "attacker", 7767033).unwrap();
    let r_shares = reference.deposit("attacker", 7767033).unwrap();
    println!("Vault shares: {}, Reference shares: {}", v_shares, r_shares);
    println!("Vault: deposited={}, shares={}", vault.total_deposited, vault.total_lp_supply);
    println!("Reference: assets={}, shares={}", reference.total_assets, reference.total_shares);

    // Action 2: AdminWithdraw 1000000
    println!("\n=== Action 2: AdminWithdraw 1000000 ===");
    execute_admin_withdraw(&mut vault, 1000000).unwrap();
    reference.admin_withdraw(1000000).unwrap();
    println!("Vault: deposited={}, shares={}, balance={}", 
        vault.total_deposited, vault.total_lp_supply, vault.stablecoin_balance);
    println!("Reference: assets={}, shares={}, admin_withdrawn={}", 
        reference.total_assets, reference.total_shares, reference.admin_withdrawn);

    // Action 3: WithdrawAll attacker
    println!("\n=== Action 3: WithdrawAll attacker ===");
    let shares = query_lp_balance(&vault, "attacker");
    println!("Attacker shares: {}", shares);
    
    let v_result = execute_withdraw(&mut vault, "attacker", shares);
    let r_result = reference.withdraw("attacker", shares);
    println!("Vault result: {:?}", v_result);
    println!("Reference result: {:?}", r_result);
    println!("Vault: deposited={}, shares={}, pending={}", 
        vault.total_deposited, vault.total_lp_supply, vault.total_pending_withdrawals);
    println!("Reference: assets={}, shares={}", reference.total_assets, reference.total_shares);

    // Action 4: Deposit user_b 1000000
    println!("\n=== Action 4: Deposit user_b 1000000 ===");
    let v_shares = execute_deposit(&mut vault, "user_b", 1000000).unwrap();
    let r_shares = reference.deposit("user_b", 1000000).unwrap();
    println!("Vault shares: {}, Reference shares: {}", v_shares, r_shares);
    println!("Vault: deposited={}, shares={}", vault.total_deposited, vault.total_lp_supply);
    println!("Reference: assets={}, shares={}", reference.total_assets, reference.total_shares);

    // Action 5: Deposit user_a 1000000
    println!("\n=== Action 5: Deposit user_a 1000000 ===");
    let v_shares = execute_deposit(&mut vault, "user_a", 1000000).unwrap();
    let r_shares = reference.deposit("user_a", 1000000).unwrap();
    println!("Vault shares: {}, Reference shares: {}", v_shares, r_shares);
    println!("Vault: deposited={}, shares={}", vault.total_deposited, vault.total_lp_supply);
    println!("Reference: assets={}, shares={}", reference.total_assets, reference.total_shares);
    assert_eq!(v_shares.u128(), r_shares, "Action 5 shares diverged!");
}
