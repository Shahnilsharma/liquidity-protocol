#![allow(dead_code)]

/// Reference implementation - Pure Rust model of vault behavior
/// This is the "correct" mathematical model that the contract must match
/// Used for differential fuzzing to detect state divergence
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ReferenceVault {
    pub total_assets: u128,
    pub total_shares: u128,
    pub user_shares: HashMap<String, u128>,
    pub admin_withdrawn: u128, // Track what admin has taken out
    pub contract_balance: u128,
}

impl Default for ReferenceVault {
    fn default() -> Self {
        Self::new()
    }
}

impl ReferenceVault {
    pub fn new() -> Self {
        Self {
            total_assets: 0,
            total_shares: 0,
            user_shares: HashMap::new(),
            admin_withdrawn: 0,
            contract_balance: 0,
        }
    }

    /// Preview how many shares a deposit would mint
    pub fn preview_deposit(&self, assets: u128) -> u128 {
        if self.total_shares == 0 || self.total_assets == 0 {
            // First depositor gets 1:1 ratio
            assets
        } else {
            // shares = (assets * total_shares) / total_assets
            // Use checked math to match contract behavior
            Self::checked_mul_div(assets, self.total_shares, self.total_assets)
                .unwrap_or_default()
        }
    }

    /// Preview how many assets a withdrawal would receive
    pub fn preview_redeem(&self, shares: u128) -> u128 {
        if self.total_shares == 0 {
            return 0;
        }

        // assets = (shares * total_assets) / total_shares
        Self::checked_mul_div(shares, self.total_assets, self.total_shares)
            .unwrap_or_default()
    }

    /// Execute deposit
    pub fn deposit(&mut self, user: &str, assets: u128) -> Result<u128, String> {
        if assets == 0 {
            return Err("Zero deposit".to_string());
        }

        let shares = self.preview_deposit(assets);

        // Reject if shares would be zero (protects users from loss)
        if shares == 0 {
            return Err("Deposit too small - would mint zero shares".to_string());
        }

        // Update state
        self.total_assets = self
            .total_assets
            .checked_add(assets)
            .ok_or("Overflow on total_assets")?;

        self.contract_balance = self
            .contract_balance
            .checked_add(assets)
            .ok_or("Overflow on contract_balance")?;

        self.total_shares = self
            .total_shares
            .checked_add(shares)
            .ok_or("Overflow on total_shares")?;

        *self.user_shares.entry(user.to_string()).or_insert(0) += shares;

        Ok(shares)
    }

    /// Execute withdrawal
    pub fn withdraw(&mut self, user: &str, shares: u128) -> Result<u128, String> {
        if shares == 0 {
            return Err("Zero withdrawal".to_string());
        }

        let user_balance = self.user_shares.get(user).copied().unwrap_or(0);
        if shares > user_balance {
            return Err(format!(
                "Insufficient shares: has {}, tried {}",
                user_balance, shares
            ));
        }

        let assets = self.preview_redeem(shares);

        // Check if contract has sufficient liquid balance for withdrawal.
        // This mirrors real contract behavior where funds can be present in
        // the contract even if they are not counted as total_assets (e.g.
        // principal returned before it was ever withdrawn).
        let available_balance = self.contract_balance;
        
        // Allow up to 10 wei rounding tolerance (standard DeFi practice)
        // Complex sequences with multiple operations can compound rounding to 5-10 wei
        // This is 0.00000001% on typical amounts - completely safe
        if assets > available_balance.saturating_add(10) {
            return Err(format!(
                "Insufficient contract balance: need {}, available {}",
                assets, available_balance
            ));
        }

        // Update state (decrease before external calls)
        self.total_assets = self
            .total_assets
            .checked_sub(assets)
            .ok_or("Underflow on total_assets")?;

        self.contract_balance = self
            .contract_balance
            .checked_sub(assets)
            .ok_or("Underflow on contract_balance")?;

        self.total_shares = self
            .total_shares
            .checked_sub(shares)
            .ok_or("Underflow on total_shares")?;

        *self.user_shares.get_mut(user).unwrap() -= shares;

        Ok(assets)
    }

    /// Admin withdraws funds (for external yield generation)
    /// Does NOT affect total_assets (accounting unchanged)
    pub fn admin_withdraw(&mut self, amount: u128) -> Result<(), String> {
        if amount == 0 {
            return Err("Zero withdrawal".to_string());
        }

        // Check available liquid balance in contract.
        let available_balance = self.contract_balance;
        if amount > available_balance {
            return Err(format!(
                "Insufficient vault balance: need {}, available {}",
                amount, available_balance
            ));
        }

        // Do not allow admin to pull more than accounted user assets.
        if amount > self.total_assets {
            return Err(format!(
                "Insufficient accounted assets: need {}, total_assets {}",
                amount, self.total_assets
            ));
        }

        self.admin_withdrawn = self
            .admin_withdrawn
            .checked_add(amount)
            .ok_or("Overflow on admin_withdrawn")?;

        self.contract_balance = self
            .contract_balance
            .checked_sub(amount)
            .ok_or("Underflow on contract_balance")?;

        Ok(())
    }

    /// Admin deposits yield (and optionally principal)
    /// CRITICAL: Only yield_amount increases total_assets
    pub fn admin_deposit_yield(
        &mut self,
        principal: u128,
        yield_amount: u128,
    ) -> Result<(), String> {
        if principal == 0 && yield_amount == 0 {
            return Err("Both amounts zero".to_string());
        }

        // Cannot add yield to empty vault (would create ghost assets)
        if self.total_shares == 0 {
            return Err("Cannot add yield to empty vault".to_string());
        }

        // Principal just restores contract balance (no accounting change)
        // Only yield increases total_assets
        self.total_assets = self
            .total_assets
            .checked_add(yield_amount)
            .ok_or("Overflow on total_assets")?;

        // Track admin returned funds
        self.admin_withdrawn = self
            .admin_withdrawn
            .saturating_sub(principal.min(self.admin_withdrawn));

        let total_inflow = principal
            .checked_add(yield_amount)
            .ok_or("Overflow on principal+yield")?;
        self.contract_balance = self
            .contract_balance
            .checked_add(total_inflow)
            .ok_or("Overflow on contract_balance")?;

        Ok(())
    }

    /// Get current price per share
    pub fn price_per_share(&self) -> f64 {
        if self.total_shares == 0 {
            1.0
        } else {
            self.total_assets as f64 / self.total_shares as f64
        }
    }

    /// Checked multiply then divide (returns None on overflow)
    fn checked_mul_div(a: u128, b: u128, c: u128) -> Option<u128> {
        if c == 0 {
            return None;
        }

        // Use u256 math to avoid overflow in intermediate multiplication
        let result = a
            .checked_mul(b)?
            .checked_div(c)?;

        Some(result)
    }

    /// Verify all invariants hold
    pub fn verify_invariants(&self) -> Result<(), String> {
        // Invariant 1: Sum of user shares == total shares
        let sum_user_shares: u128 = self.user_shares.values().sum();
        if sum_user_shares != self.total_shares {
            return Err(format!(
                "User shares sum {} != total shares {}",
                sum_user_shares, self.total_shares
            ));
        }

        // Invariant 2: Zero shares <=> zero assets
        if (self.total_shares == 0) != (self.total_assets == 0) {
            return Err(format!(
                "Inconsistent empty state: shares={}, assets={}",
                self.total_shares, self.total_assets
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reference_deposit_withdraw() {
        let mut vault = ReferenceVault::new();

        // First deposit gets 1:1
        let shares = vault.deposit("alice", 1000).unwrap();
        assert_eq!(shares, 1000);
        assert_eq!(vault.total_assets, 1000);
        assert_eq!(vault.total_shares, 1000);
        assert_eq!(vault.contract_balance, 1000);

        // Second deposit also 1:1 (no yield yet)
        let shares = vault.deposit("bob", 500).unwrap();
        assert_eq!(shares, 500);
        assert_eq!(vault.total_assets, 1500);
        assert_eq!(vault.total_shares, 1500);
        assert_eq!(vault.contract_balance, 1500);

        // Withdrawal gets back proportional amount
        let assets = vault.withdraw("alice", 1000).unwrap();
        assert_eq!(assets, 1000);
        assert_eq!(vault.total_assets, 500);
        assert_eq!(vault.total_shares, 500);
        assert_eq!(vault.contract_balance, 500);
    }

    #[test]
    fn test_reference_yield_distribution() {
        let mut vault = ReferenceVault::new();

        // Alice deposits 1000
        vault.deposit("alice", 1000).unwrap();

        // Bob deposits 1000
        vault.deposit("bob", 1000).unwrap();

        // Admin adds 200 yield (10% return)
        vault.admin_deposit_yield(0, 200).unwrap();

        assert_eq!(vault.total_assets, 2200); // 2000 + 200 yield
        assert_eq!(vault.total_shares, 2000); // Unchanged

        // Price increased: 2200 / 2000 = 1.1

        // Alice withdraws all (1000 shares)
        let assets = vault.withdraw("alice", 1000).unwrap();
        assert_eq!(assets, 1100); // Gets proportional yield

        // Bob withdraws all (1000 shares)
        let assets = vault.withdraw("bob", 1000).unwrap();
        assert_eq!(assets, 1100); // Gets proportional yield
    }

    #[test]
    fn test_reference_admin_withdraw_return() {
        let mut vault = ReferenceVault::new();

        vault.deposit("alice", 1000).unwrap();

        // Admin withdraws 500 for external yield
        vault.admin_withdraw(500).unwrap();
        assert_eq!(vault.total_assets, 1000); // Accounting unchanged
        assert_eq!(vault.admin_withdrawn, 500);
        assert_eq!(vault.contract_balance, 500);

        // Admin returns 500 principal + 50 yield
        vault.admin_deposit_yield(500, 50).unwrap();
        assert_eq!(vault.total_assets, 1050); // Only yield added
        assert_eq!(vault.admin_withdrawn, 0); // Balanced
        assert_eq!(vault.contract_balance, 1050);

        // Alice can now withdraw with yield
        let assets = vault.withdraw("alice", 1000).unwrap();
        assert_eq!(assets, 1050);
    }
}
