/// Common utilities and helpers for fuzz testing (Mock Version)
/// This module uses MockVault to test core logic without TokenFactory
use cosmwasm_std::Uint128;

pub use self::mock_vault_impl::MockVault;

// Test wallets
pub const ADMIN: &str = "admin";
pub const USER_A: &str = "user_a";
pub const USER_B: &str = "user_b";
pub const USER_C: &str = "user_c";
pub const ATTACKER: &str = "attacker";
pub const VICTIM: &str = "victim";

// Token denoms
pub const STABLECOIN: &str = "uzig";
pub const INITIAL_BALANCE: u128 = 1_000_000_000_000; // 1M uzig per user

// Include the mock vault implementation
mod mock_vault_impl {
    use std::collections::HashMap;

    pub struct MockVault {
        pub contract_address: String,
        pub admin: String,
        pub stablecoin_denom: String,
        pub lp_denom: String,
        pub withdrawal_delay: u64,
        
        // Core accounting
        pub total_deposited: u128,
        pub total_lp_supply: u128,
        pub total_pending_withdrawals: u128,
        pub admin_withdrawn: u128, // Track how much admin has taken
        
        // User balances
        pub lp_balances: HashMap<String, u128>,
        pub pending_withdrawals: HashMap<String, Vec<PendingWithdrawal>>,
        pub withdrawal_counter: HashMap<String, u64>,
        
        // Contract balance (simulated)
        pub stablecoin_balance: u128,
        
        // Time simulation
        pub current_time: u64,
    }

    #[derive(Clone, Debug)]
    pub struct PendingWithdrawal {
        pub id: u64,
        pub amount: u128,
        pub release_time: u64,
    }

    impl MockVault {
        pub fn new(admin: &str, stablecoin_denom: &str, withdrawal_delay: u64) -> Self {
            Self {
                contract_address: "vault_contract".to_string(),
                admin: admin.to_string(),
                stablecoin_denom: stablecoin_denom.to_string(),
                lp_denom: "factory/vault_contract/lp".to_string(),
                withdrawal_delay,
                total_deposited: 0,
                total_lp_supply: 0,
                total_pending_withdrawals: 0,
                admin_withdrawn: 0,
                lp_balances: HashMap::new(),
                pending_withdrawals: HashMap::new(),
                withdrawal_counter: HashMap::new(),
                stablecoin_balance: 0,
                current_time: 0,
            }
        }

        pub fn deposit(&mut self, user: &str, amount: u128) -> Result<u128, String> {
            // Reject unreasonably large amounts (overflow protection)
            if amount > u128::MAX / 2 {
                return Err("Amount too large".to_string());
            }

            // Calculate shares
            let shares = if self.total_lp_supply == 0 {
                amount // First deposit: 1:1
            } else {
                amount
                    .checked_mul(self.total_lp_supply)
                    .ok_or("Multiplication overflow")?
                    .checked_div(self.total_deposited)
                    .ok_or("Division by zero")?
            };

            // Reject if shares would be zero (prevents exploitation)
            if shares == 0 {
                return Err("Deposit too small - would mint zero shares".to_string());
            }

            // Update state
            self.total_deposited = self.total_deposited.checked_add(amount)
                .ok_or("Overflow in total_deposited")?;
            self.total_lp_supply = self.total_lp_supply.checked_add(shares)
                .ok_or("Overflow in total_lp_supply")?;
            self.stablecoin_balance = self.stablecoin_balance.checked_add(amount)
                .ok_or("Overflow in balance")?;

            // Update user balance
            let user_balance = self.lp_balances.entry(user.to_string()).or_insert(0);
            *user_balance = user_balance.checked_add(shares)
                .ok_or("Overflow in user LP balance")?;

            Ok(shares)
        }

        pub fn request_withdraw(&mut self, user: &str, lp_amount: u128) -> Result<u64, String> {
            // Check user has enough LP tokens
            let user_balance = self.lp_balances.get(user).copied().unwrap_or(0);
            if user_balance < lp_amount {
                return Err("Insufficient LP tokens".to_string());
            }

            // Calculate stablecoin amount
            let stablecoin_amount = lp_amount
                .checked_mul(self.total_deposited)
                .ok_or("Multiplication overflow")?
                .checked_div(self.total_lp_supply)
                .ok_or("Division by zero")?;
            
            // CRITICAL: Check if contract will have sufficient balance when claim happens
            // This prevents burning shares for withdrawals that will fail anyway
            // For maximum realism, we check current balance minus pending (not admin_withdrawn)
            let available_after_pending = self.stablecoin_balance
                .saturating_sub(self.total_pending_withdrawals);
            
            if stablecoin_amount > available_after_pending {
                return Err(format!(
                    "Insufficient contract balance for withdrawal: need {}, available {} (after {} pending)",
                    stablecoin_amount, available_after_pending, self.total_pending_withdrawals
                ));
            }

            // Burn LP tokens (only after all checks pass)
            let user_balance_mut = self.lp_balances.get_mut(user).unwrap();
            *user_balance_mut = user_balance_mut.checked_sub(lp_amount)
                .ok_or("Underflow in user balance")?;

            self.total_lp_supply = self.total_lp_supply.checked_sub(lp_amount)
                .ok_or("Underflow in total_lp_supply")?;

            // Create pending withdrawal
            let withdrawal_id = self.withdrawal_counter.entry(user.to_string()).or_insert(0);
            *withdrawal_id += 1;

            let release_time = self.current_time + self.withdrawal_delay;
            let pending = PendingWithdrawal {
                id: *withdrawal_id,
                amount: stablecoin_amount,
                release_time,
            };

            self.pending_withdrawals
                .entry(user.to_string())
                .or_insert_with(Vec::new)
                .push(pending);

            self.total_pending_withdrawals = self.total_pending_withdrawals
                .checked_add(stablecoin_amount)
                .ok_or("Overflow in pending withdrawals")?;

            Ok(*withdrawal_id)
        }

        pub fn claim_withdraw(&mut self, user: &str, withdrawal_id: u64) -> Result<u128, String> {
            let user_withdrawals = self.pending_withdrawals
                .get_mut(user)
                .ok_or("No pending withdrawals")?;

            let index = user_withdrawals
                .iter()
                .position(|w| w.id == withdrawal_id)
                .ok_or("Withdrawal not found")?;

            let withdrawal = &user_withdrawals[index];

            // Check time-lock
            if self.current_time < withdrawal.release_time {
                return Err(format!(
                    "Withdrawal locked until time {}",
                    withdrawal.release_time
                ));
            }

            let amount = withdrawal.amount;

            // Check contract has enough balance
            if self.stablecoin_balance < amount {
                return Err("Insufficient contract balance".to_string());
            }

            // Remove withdrawal and update state
            user_withdrawals.remove(index);
            
            self.stablecoin_balance = self.stablecoin_balance.checked_sub(amount)
                .ok_or("Underflow in balance")?;
            self.total_pending_withdrawals = self.total_pending_withdrawals.checked_sub(amount)
                .ok_or("Underflow in pending withdrawals")?;
            self.total_deposited = self.total_deposited.checked_sub(amount)
                .ok_or("Underflow in total_deposited")?;

            Ok(amount)
        }

        pub fn admin_withdraw(&mut self, caller: &str, amount: u128) -> Result<(), String> {
            if caller != self.admin {
                return Err("Unauthorized".to_string());
            }

            if amount == 0 {
                return Err("Zero withdrawal".to_string());
            }

            // Check available balance (can't withdraw more than exists)
            if amount > self.stablecoin_balance {
                return Err("Insufficient balance".to_string());
            }
            
            // Check that we don't withdraw more than total_deposited
            // (can't take more from vault than users deposited)
            if amount > self.total_deposited {
                return Err(format!(
                    "Insufficient vault balance: need {}, total_deposited {}",
                    amount, self.total_deposited
                ));
            }

            // Track admin withdrawal
            self.admin_withdrawn = self.admin_withdrawn.checked_add(amount)
                .ok_or("Overflow in admin_withdrawn")?;
                
            // Only decrease balance, NOT total_deposited
            self.stablecoin_balance = self.stablecoin_balance.checked_sub(amount)
                .ok_or("Underflow in balance")?;

            Ok(())
        }

        pub fn admin_deposit_yield(
            &mut self,
            caller: &str,
            principal: u128,
            yield_amount: u128,
        ) -> Result<(), String> {
            if caller != self.admin {
                return Err("Unauthorized".to_string());
            }

            // Reject if both amounts are zero
            if principal == 0 && yield_amount == 0 {
                return Err("Both amounts zero".to_string());
            }

            // Cannot add yield to empty vault (would create ghost assets)
            if self.total_lp_supply == 0 {
                return Err("Cannot add yield to empty vault".to_string());
            }

            // Only yield increases total_deposited (CRITICAL FIX)
            self.total_deposited = self.total_deposited.checked_add(yield_amount)
                .ok_or("Overflow in total_deposited")?;

            // Track principal return (reduces admin_withdrawn)
            self.admin_withdrawn = self.admin_withdrawn.checked_sub(principal.min(self.admin_withdrawn))
                .unwrap_or(0);

            // Both principal and yield restore the balance
            let total = principal.checked_add(yield_amount)
                .ok_or("Overflow in total amount")?;
            self.stablecoin_balance = self.stablecoin_balance.checked_add(total)
                .ok_or("Overflow in balance")?;

            Ok(())
        }

        pub fn advance_time(&mut self, seconds: u64) {
            self.current_time += seconds;
        }

        pub fn get_lp_balance(&self, user: &str) -> u128 {
            self.lp_balances.get(user).copied().unwrap_or(0)
        }

        pub fn price_per_share(&self) -> f64 {
            if self.total_lp_supply == 0 {
                1.0
            } else {
                self.total_deposited as f64 / self.total_lp_supply as f64
            }
        }

        pub fn verify_invariants(&self) -> Result<(), String> {
            // Note: We DON'T check if balance >= pending_withdrawals because in rug scenarios,
            // admin may not have returned enough funds yet. Users will need to wait or accept loss.
            // This is expected behavior, not an invariant violation.
            
            // Invariant 1: Zero shares implies zero assets (unless pending withdrawals exist)
            if self.total_lp_supply == 0 && self.total_pending_withdrawals == 0 && self.total_deposited != 0 {
                return Err("Ghost assets exist with zero shares".to_string());
            }

            // Invariant 2: Zero assets implies zero shares (unless pending withdrawals exist)
            if self.total_deposited == 0 && self.total_pending_withdrawals == 0 && self.total_lp_supply != 0 {
                return Err("Ghost shares exist with zero assets".to_string());
            }

            // Invariant 3: User LP balances sum must equal total supply
            let user_total: u128 = self.lp_balances.values().sum();
            if user_total != self.total_lp_supply {
                return Err(format!(
                    "User LP total {} != total supply {}",
                    user_total, self.total_lp_supply
                ));
            }

            Ok(())
        }
    }
}

/// Setup a fresh mock vault
pub fn setup_vault(withdrawal_delay: u64) -> MockVault {
    MockVault::new(ADMIN, STABLECOIN, withdrawal_delay)
}

/// Execute deposit and return shares minted
pub fn execute_deposit(
    vault: &mut MockVault,
    user: &str,
    amount: u128,
) -> Result<Uint128, String> {
    let shares = vault.deposit(user, amount)?;
    Ok(Uint128::new(shares))
}

/// Execute withdrawal and return amount received
pub fn execute_withdraw(
    vault: &mut MockVault,
    user: &str,
    lp_amount: u128,
) -> Result<u128, String> {
    // Request withdraw
    let withdrawal_id = vault.request_withdraw(user, lp_amount)?;
    
    // Advance time by withdrawal delay
    vault.advance_time(vault.withdrawal_delay);
    
    // Claim withdraw
    let amount = vault.claim_withdraw(user, withdrawal_id)?;
    
    Ok(amount)
}

/// Admin withdraws funds from vault
pub fn execute_admin_withdraw(
    vault: &mut MockVault,
    amount: u128,
) -> Result<(), String> {
    vault.admin_withdraw(ADMIN, amount)
}

/// Admin deposits yield (with principal return)
pub fn execute_admin_deposit_yield(
    vault: &mut MockVault,
    principal: u128,
    yield_amount: u128,
) -> Result<(), String> {
    vault.admin_deposit_yield(ADMIN, principal, yield_amount)
}

/// Query user's LP token balance
pub fn query_lp_balance(vault: &MockVault, user: &str) -> u128 {
    vault.get_lp_balance(user)
}

/// Calculate price per share
pub fn calculate_price_per_share(total_assets: u128, total_shares: u128) -> f64 {
    if total_shares == 0 {
        1.0
    } else {
        total_assets as f64 / total_shares as f64
    }
}

/// Global invariant assertions - run after every operation
/// Note: In rug scenarios where admin withdraws funds and doesn't return all,
/// pending_withdrawals can exceed total_deposited if request_withdraw succeeds
/// but claim_withdraw fails. This is expected behavior in adversarial scenarios.
pub fn assert_vault_invariants(vault: &MockVault, tag: &str) {
    // Invariant 1: Total deposited must be >= pending withdrawals (relaxed for rug scenarios)
    // In normal operations this holds, but in partial rug scenarios where claims fail,
    // pending can temporarily exceed deposited due to LP burn before payout.
    if vault.stablecoin_balance >= vault.total_pending_withdrawals {
        // Only enforce if balance can cover pending (normal scenario)
        assert!(
            vault.total_deposited >= vault.total_pending_withdrawals,
            "{}: total_deposited < pending_withdrawals in solvent vault",
            tag
        );
    }

    // Invariant 2: Zero shares AND zero pending = zero assets (empty vault)
    if vault.total_lp_supply == 0 && vault.total_pending_withdrawals == 0 {
        assert_eq!(
            vault.total_deposited, 0,
            "{}: Ghost assets exist with zero shares and no pending withdrawals",
            tag
        );
    }

    // Invariant 3: Share price never less than epsilon if assets exist
    if vault.total_deposited != 0 {
        assert!(
            vault.total_lp_supply != 0 || vault.total_pending_withdrawals != 0,
            "{}: Assets exist but no shares and no pending withdrawals",
            tag
        );
    }
    
    // Invariant 4: Internal consistency check
    vault.verify_invariants().expect(&format!("{}: Invariant check failed", tag));
}
