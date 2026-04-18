// Debug latest failing sequence
mod fuzz_helpers;
mod reference_model;

use fuzz_helpers::*;
use reference_model::ReferenceVault;

#[test]
fn debug_latest_fail() {
    let mut vault = setup_vault(120);
    let mut reference = ReferenceVault::new();
    
    println!("=== Action 1: Deposit user_b 2337169 ===");
    execute_deposit(&mut vault, "user_b", 2337169).unwrap();
    reference.deposit("user_b", 2337169).unwrap();
    println!("Vault: dep={}, shares={}, balance={}", vault.total_deposited, vault.total_lp_supply, vault.stablecoin_balance);
    println!("Ref: assets={}, shares={}, withdrawn={}", reference.total_assets, reference.total_shares, reference.admin_withdrawn);

    println!("\n=== Action 2: Deposit user_a 1000000 ===");
    execute_deposit(&mut vault, "user_a", 1000000).unwrap();
    reference.deposit("user_a", 1000000).unwrap();
    println!("Vault: dep={}, shares={}, balance={}", vault.total_deposited, vault.total_lp_supply, vault.stablecoin_balance);
    println!("Ref: assets={}, shares={}, withdrawn={}", reference.total_assets, reference.total_shares, reference.admin_withdrawn);

    println!("\n=== Action 3: AdminDepositYield principal=817438 yield=1523898 ===");
    let v3 = execute_admin_deposit_yield(&mut vault, 817438, 1523898);
    let r3 = reference.admin_deposit_yield(817438, 1523898);
    println!("Vault result: {:?}", v3);
    println!("Ref result: {:?}", r3);
    println!("Vault: dep={}, shares={}, balance={}", vault.total_deposited, vault.total_lp_supply, vault.stablecoin_balance);
    println!("Ref: assets={}, shares={}, withdrawn={}", reference.total_assets, reference.total_shares, reference.admin_withdrawn);

    println!("\n=== Action 4: AdminWithdraw 4861068 ===");
    let v4 = execute_admin_withdraw(&mut vault, 4861068);
    let r4 = reference.admin_withdraw(4861068);
    println!("Vault result: {:?}", v4);
    println!("Ref result: {:?}", r4);
    println!("Vault: dep={}, shares={}, balance={}", vault.total_deposited, vault.total_lp_supply, vault.stablecoin_balance);
    println!("Ref: assets={}, shares={}, withdrawn={}", reference.total_assets, reference.total_shares, reference.admin_withdrawn);

    println!("\n=== Action 5: WithdrawAll user_a ===");
    let shares = query_lp_balance(&vault, "user_a");
    println!("User A shares: {}", shares);
    
    let v5 = execute_withdraw(&mut vault, "user_a", shares);
    let r5 = reference.withdraw("user_a", shares);
    println!("Vault result: {:?}", v5);
    println!("Ref result: {:?}", r5);
    println!("Vault: dep={}, shares={}, balance={}, pending={}", 
        vault.total_deposited, vault.total_lp_supply, vault.stablecoin_balance, vault.total_pending_withdrawals);
    println!("Ref: assets={}, shares={}, withdrawn={}", reference.total_assets, reference.total_shares, reference.admin_withdrawn);
    
    // Check if both agree on success/failure
    match (v5, r5) {
        (Ok(_), Ok(_)) => println!("Both succeeded"),
        (Err(_), Err(_)) => println!("Both failed"),
        (Ok(_), Err(_)) => panic!("Vault succeeded but Reference failed!"),
        (Err(_), Ok(_)) => panic!("Vault failed but Reference succeeded!"),
    }
}
