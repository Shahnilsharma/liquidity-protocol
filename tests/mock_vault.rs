/// Mock Vault for Testing Without TokenFactory
/// This allows fuzz tests to run without blockchain-specific TokenFactory integration
/// Core vault logic is tested, TokenFactory is mocked
use std::collections::HashMap;

/// Mock vault that tracks state without actual contract calls
#[derive(Debug, Clone)]
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
    
    // User balances
    pub lp_balances: HashMap<String, u128>,
    pub pending_withdrawals: HashMap<String, Vec<PendingWithdrawal>>,
    pub withdrawal_counter: HashMap<String, u64>,
    
    // Contract balance
    pub stablecoin_balance: u128,
    
    // Time simulation
    pub current_time: u64,
}

#[derive(Debug, Clone)]
pub struct PendingWithdrawal {
    pub id: u64,
    pub amount: u128,
    pub release_time: u64,
}

impl MockVault {
    pub fn new(admin: &str, stablecoin_denom: &str, withdrawal_delay: u64) -> Self {
        Self {
            contract_address: "mock_vault".to_string(),
            admin: admin.to_string(),
            stablecoin_denom: stablecoin_denom.to_string(),
            lp_denom: "mock_lp".to_string(),
            withdrawal_delay,
            total_deposited: 0,
            total_lp_supply: 0,
            total_pending_withdrawals: 0,
            lp_balances: HashMap::new(),
            pending_withdrawals: HashMap::new(),
            withdrawal_counter: HashMap::new(),
            stablecoin_balance: 0,
            current_time: 1_000_000,
        }
    }
    
    /// Deposit stablecoins, receive LP tokens
    pub fn deposit(&mut self, user: &str, amount: u128) -> Result<u128, String> {
        if amount == 0 {
            return Err("Zero deposit".to_string());
        }
        
        // Calculate shares to mint
        let shares = if self.total_lp_supply == 0 || self.total_deposited == 0 {
            amount // 1:1 for first deposit
        } else {
            // shares = (amount * total_lp_supply) / total_deposited
            amount
                .checked_mul(self.total_lp_supply)
                .and_then(|r| r.checked_div(self.total_deposited))
                .ok_or("Overflow in share calculation")?
        };
        
        // Reject if shares would be zero
        if shares == 0 {
            return Err("Deposit too small - would mint zero shares".to_string());
        }
        
        // Update state
        self.total_deposited = self.total_deposited.checked_add(amount)
            .ok_or("Overflow on total_deposited")?;
        self.total_lp_supply = self.total_lp_supply.checked_add(shares)
            .ok_or("Overflow on total_lp_supply")?;
        self.stablecoin_balance = self.stablecoin_balance.checked_add(amount)
            .ok_or("Overflow on balance")?;
        
        *self.lp_balances.entry(user.to_string()).or_insert(0) += shares;
        
        Ok(shares)
    }
    
    /// Request withdrawal - burns LP tokens, creates pending withdrawal
    pub fn request_withdraw(&mut self, user: &str, lp_amount: u128) -> Result<u64, String> {
        if lp_amount == 0 {
            return Err("Zero withdrawal".to_string());
        }
        
        let user_balance = self.lp_balances.get(user).copied().unwrap_or(0);
        if lp_amount > user_balance {
            return Err(format!("Insufficient LP balance: has {}, tried {}", user_balance, lp_amount));
        }
        
        // Calculate stablecoin amount
        let stablecoin_amount = if self.total_lp_supply == 0 {
            0
        } else {
            lp_amount
                .checked_mul(self.total_deposited)
                .and_then(|r| r.checked_div(self.total_lp_supply))
                .ok_or("Overflow in withdrawal calculation")?
        };
        
        // Update state (burn LP tokens)
        self.total_lp_supply = self.total_lp_supply.checked_sub(lp_amount)
            .ok_or("Underflow on total_lp_supply")?;
        self.total_pending_withdrawals = self.total_pending_withdrawals.checked_add(stablecoin_amount)
            .ok_or("Overflow on pending_withdrawals")?;
        
        *self.lp_balances.get_mut(user).unwrap() -= lp_amount;
        
        // Create pending withdrawal
        let withdrawal_id = *self.withdrawal_counter.entry(user.to_string()).or_insert(0);
        *self.withdrawal_counter.get_mut(user).unwrap() += 1;
        
        let release_time = self.current_time + self.withdrawal_delay;
        
        self.pending_withdrawals
            .entry(user.to_string())
            .or_default()
            .push(PendingWithdrawal {
                id: withdrawal_id,
                amount: stablecoin_amount,
                release_time,
            });
        
        Ok(withdrawal_id)
    }
    
    /// Claim pending withdrawal after time-lock
    pub fn claim_withdraw(&mut self, user: &str, withdrawal_id: u64) -> Result<u128, String> {
        let withdrawals = self.pending_withdrawals
            .get_mut(user)
            .ok_or("No pending withdrawals")?;
        
        let pos = withdrawals.iter().position(|w| w.id == withdrawal_id)
            .ok_or("Withdrawal not found")?;
        
        let withdrawal = &withdrawals[pos];
        
        // Check time-lock
        if self.current_time < withdrawal.release_time {
            return Err(format!(
                "Withdrawal locked until {}. Current: {}",
                withdrawal.release_time, self.current_time
            ));
        }
        
        // Check contract has balance
        if self.stablecoin_balance < withdrawal.amount {
            return Err(format!(
                "Insufficient contract balance: {} < {}",
                self.stablecoin_balance, withdrawal.amount
            ));
        }
        
        let amount = withdrawal.amount;
        
        // Update state
        self.total_deposited = self.total_deposited.checked_sub(amount)
            .ok_or("Underflow on total_deposited")?;
        self.total_pending_withdrawals = self.total_pending_withdrawals.checked_sub(amount)
            .ok_or("Underflow on pending_withdrawals")?;
        self.stablecoin_balance = self.stablecoin_balance.checked_sub(amount)
            .ok_or("Underflow on balance")?;
        
        withdrawals.remove(pos);
        
        Ok(amount)
    }
    
    /// Admin withdraws funds
    pub fn admin_withdraw(&mut self, caller: &str, amount: u128) -> Result<(), String> {
        if caller != self.admin {
            return Err("Unauthorized".to_string());
        }
        
        if amount == 0 {
            return Err("Zero amount".to_string());
        }
        
        if self.stablecoin_balance < amount {
            return Err("Insufficient balance".to_string());
        }
        
        // Decrease balance but NOT total_deposited (by design)
        self.stablecoin_balance = self.stablecoin_balance.checked_sub(amount)
            .ok_or("Underflow")?;
        
        Ok(())
    }
    
    /// Admin deposits yield (and optionally returns principal)
    pub fn admin_deposit_yield(&mut self, caller: &str, principal: u128, yield_amount: u128) -> Result<(), String> {
        if caller != self.admin {
            return Err("Unauthorized".to_string());
        }
        
        if principal == 0 && yield_amount == 0 {
            return Err("Both amounts zero".to_string());
        }
        
        let total = principal.checked_add(yield_amount)
            .ok_or("Overflow")?;
        
        // Only yield increases total_deposited (principal just restores balance)
        self.total_deposited = self.total_deposited.checked_add(yield_amount)
            .ok_or("Overflow on total_deposited")?;
        
        self.stablecoin_balance = self.stablecoin_balance.checked_add(total)
            .ok_or("Overflow on balance")?;
        
        Ok(())
    }
    
    /// Advance time for testing
    pub fn advance_time(&mut self, seconds: u64) {
        self.current_time += seconds;
    }
    
    /// Get user LP balance
    pub fn get_lp_balance(&self, user: &str) -> u128 {
        self.lp_balances.get(user).copied().unwrap_or(0)
    }
    
    /// Get price per share
    pub fn price_per_share(&self) -> f64 {
        if self.total_lp_supply == 0 {
            1.0
        } else {
            self.total_deposited as f64 / self.total_lp_supply as f64
        }
    }
    
    /// Check invariants
    pub fn verify_invariants(&self) -> Result<(), String> {
        // Invariant 1: Sum of user LP balances == total supply
        let sum_lp: u128 = self.lp_balances.values().sum();
        if sum_lp != self.total_lp_supply {
            return Err(format!("LP supply mismatch: sum={}, total={}", sum_lp, self.total_lp_supply));
        }
        
        // Invariant 2: Zero shares iff zero assets
        if (self.total_lp_supply == 0) != (self.total_deposited == 0) {
            return Err(format!(
                "Empty state inconsistent: shares={}, assets={}",
                self.total_lp_supply, self.total_deposited
            ));
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_mock_deposit_withdraw() {
        let mut vault = MockVault::new("admin", "uzig", 7200);
        
        // Alice deposits 1000
        let shares = vault.deposit("alice", 1000).unwrap();
        assert_eq!(shares, 1000); // 1:1 first deposit
        assert_eq!(vault.total_deposited, 1000);
        assert_eq!(vault.get_lp_balance("alice"), 1000);
        
        // Request withdrawal
        let withdrawal_id = vault.request_withdraw("alice", 1000).unwrap();
        assert_eq!(vault.get_lp_balance("alice"), 0); // LP burned
        
        // Try early claim (should fail)
        let result = vault.claim_withdraw("alice", withdrawal_id);
        assert!(result.is_err());
        
        // Advance time
        vault.advance_time(7200);
        
        // Claim now works
        let amount = vault.claim_withdraw("alice", withdrawal_id).unwrap();
        assert_eq!(amount, 1000);
        
        vault.verify_invariants().unwrap();
    }
    
    #[test]
    fn test_mock_yield() {
        let mut vault = MockVault::new("admin", "uzig", 7200);
        
        vault.deposit("alice", 1000).unwrap();
        assert_eq!(vault.price_per_share(), 1.0);
        
        // Admin adds 100 yield
        vault.admin_deposit_yield("admin", 0, 100).unwrap();
        assert_eq!(vault.total_deposited, 1100);
        assert_eq!(vault.price_per_share(), 1.1);
        
        vault.verify_invariants().unwrap();
    }
}
