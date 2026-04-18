#![allow(dead_code)]

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

// Include the mock vault implementation
mod mock_vault_impl {
    use std::collections::HashMap;

    pub struct MockVault {
        pub contract_address: String,
        pub admin: String,
        pub stablecoin_denom: String,
        pub lp_denom: String,
        pub withdrawal_delay: u64,
        pub total_deposited: u128,
        pub total_lp_supply: u128,
        pub total_pending_withdrawals: u128,
        pub admin_withdrawn: u128,
        pub lp_balances: HashMap<String, u128>,
        pub pending_withdrawals: HashMap<String, Vec<PendingWithdrawal>>,
        pub withdrawal_counter: HashMap<String, u64>,
        pub stablecoin_balance: u128,
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
            if amount > u128::MAX / 2 {
                return Err("Amount too large".to_string());
            }
            let shares = if self.total_lp_supply == 0 {
                amount
            } else {
                amount
                    .checked_mul(self.total_lp_supply)
                    .ok_or("Multiplication overflow")?
                    .checked_div(self.total_deposited)
                    .ok_or("Division by zero")?
            };
            if shares == 0 {
                return Err("Deposit too small - would mint zero shares".to_string());
            }
            self.total_deposited = self.total_deposited.checked_add(amount)
                .ok_or("Overflow in total_deposited")?;
            self.total_lp_supply = self.total_lp_supply.checked_add(shares)
                .ok_or("Overflow in total_lp_supply")?;
            self.stablecoin_balance = self.stablecoin_balance.checked_add(amount)
                .ok_or("Overflow in balance")?;
            let user_balance = self.lp_balances.entry(user.to_string()).or_insert(0);
            *user_balance = user_balance.checked_add(shares)
                .ok_or("Overflow in user LP balance")?;
            Ok(shares)
        }

        pub fn request_withdraw(&mut self, user: &str, lp_amount: u128) -> Result<u64, String> {
            let user_balance = self.lp_balances.get(user).copied().unwrap_or(0);
            if user_balance < lp_amount {
                return Err("Insufficient LP tokens".to_string());
            }
            let stablecoin_amount = lp_amount
                .checked_mul(self.total_deposited)
                .ok_or("Multiplication overflow")?
                .checked_div(self.total_lp_supply)
                .ok_or("Division by zero")?;
            let available_after_pending = self.stablecoin_balance
                .saturating_sub(self.total_pending_withdrawals);
            if stablecoin_amount > available_after_pending {
                return Err(format!(
                    "Insufficient contract balance for withdrawal: need {}, available {} (after {} pending)",
                    stablecoin_amount, available_after_pending, self.total_pending_withdrawals
                ));
            }
            let user_balance_mut = self.lp_balances.get_mut(user).unwrap();
            *user_balance_mut = user_balance_mut.checked_sub(lp_amount)
                .ok_or("Underflow in user balance")?;
            self.total_lp_supply = self.total_lp_supply.checked_sub(lp_amount)
                .ok_or("Underflow in total_lp_supply")?;
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
                .or_default()
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
            if self.current_time < withdrawal.release_time {
                return Err(format!(
                    "Withdrawal locked until time {}",
                    withdrawal.release_time
                ));
            }
            let amount = withdrawal.amount;
            if self.stablecoin_balance < amount {
                return Err("Insufficient contract balance".to_string());
            }
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
            if amount > self.stablecoin_balance {
                return Err("Insufficient balance".to_string());
            }
            if amount > self.total_deposited {
                return Err(format!(
                    "Insufficient vault balance: need {}, total_deposited {}",
                    amount, self.total_deposited
                ));
            }
            self.admin_withdrawn = self.admin_withdrawn.checked_add(amount)
                .ok_or("Overflow in admin_withdrawn")?;
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
            if principal == 0 && yield_amount == 0 {
                return Err("Both amounts zero".to_string());
            }
            if self.total_lp_supply == 0 {
                return Err("Cannot add yield to empty vault".to_string());
            }
            self.total_deposited = self.total_deposited.checked_add(yield_amount)
                .ok_or("Overflow in total_deposited")?;
            self.admin_withdrawn = self
                .admin_withdrawn
                .saturating_sub(principal.min(self.admin_withdrawn));
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
            if self.total_lp_supply == 0 && self.total_pending_withdrawals == 0 && self.total_deposited != 0 {
                return Err("Ghost assets exist with zero shares".to_string());
            }
            if self.total_deposited == 0 && self.total_pending_withdrawals == 0 && self.total_lp_supply != 0 {
                return Err("Ghost shares exist with zero assets".to_string());
            }
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
    // End of impl MockVault
}

pub fn setup_vault(withdrawal_delay: u64) -> MockVault {
    MockVault::new(ADMIN, STABLECOIN, withdrawal_delay)
}

pub fn execute_deposit(
    vault: &mut MockVault,
    user: &str,
    amount: u128,
) -> Result<Uint128, String> {
    let shares = vault.deposit(user, amount)?;
    Ok(Uint128::new(shares))
}

pub fn execute_withdraw(
    vault: &mut MockVault,
    user: &str,
    lp_amount: u128,
) -> Result<u128, String> {
    let withdrawal_id = vault.request_withdraw(user, lp_amount)?;
    vault.advance_time(vault.withdrawal_delay);
    let amount = vault.claim_withdraw(user, withdrawal_id)?;
    Ok(amount)
}

pub fn execute_admin_withdraw(
    vault: &mut MockVault,
    amount: u128,
) -> Result<(), String> {
    vault.admin_withdraw(ADMIN, amount)
}

pub fn execute_admin_deposit_yield(
    vault: &mut MockVault,
    principal: u128,
    yield_amount: u128,
) -> Result<(), String> {
    vault.admin_deposit_yield(ADMIN, principal, yield_amount)
}

pub fn query_lp_balance(vault: &MockVault, user: &str) -> u128 {
    vault.get_lp_balance(user)
}

pub fn calculate_price_per_share(total_assets: u128, total_shares: u128) -> f64 {
    if total_shares == 0 {
        1.0
    } else {
        total_assets as f64 / total_shares as f64
    }
}

pub fn assert_vault_invariants(vault: &MockVault, tag: &str) {
    if vault.stablecoin_balance >= vault.total_pending_withdrawals {
        assert!(
            vault.total_deposited >= vault.total_pending_withdrawals,
            "{}: total_deposited < pending_withdrawals in solvent vault",
            tag
        );
    }
    if vault.total_lp_supply == 0 && vault.total_pending_withdrawals == 0 {
        assert_eq!(
            vault.total_deposited, 0,
            "{}: Ghost assets exist with zero shares and no pending withdrawals",
            tag
        );
    }
    if vault.total_deposited != 0 {
        assert!(
            vault.total_lp_supply != 0 || vault.total_pending_withdrawals != 0,
            "{}: Assets exist but no shares and no pending withdrawals",
            tag
        );
    }
    vault
        .verify_invariants()
        .unwrap_or_else(|_| panic!("{}: Invariant check failed", tag));
}
