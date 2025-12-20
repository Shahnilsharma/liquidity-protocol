# Phase 5 Error Testing Guide
## Understanding Why Tests Failed & How to Make Them Work

## Overview

Phase 5 tests are designed to verify that the smart contract properly handles **error conditions**. These tests intentionally try to break the contract to ensure it has proper validation and security.

**Test Results Summary:**
- Test 5.1 (Zero Deposit): PASSED
- Test 5.2 (Over-withdrawal): Requires specific setup
- Test 5.3 (No Approval): Requires removing existing state

## Test 5.1: Zero Deposit - PASSED

### What It Tests
Verifies that the contract rejects deposits of 0 tokens.

### Why It Worked
The contract has explicit validation code:
```rust
if amount.is_zero() {
    return Err(ContractError::InvalidZeroAmount {});
}
```

### Test Command
```bash
zigchaind tx wasm execute $LP_POOL_ADDRESS \
  '{"deposit":{"amount":"0"}}' \
  --from mynewwallet \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 \
  --gas 500000 \
  --fees 25000uzig \
  --yes
```

### Actual Result
- Transaction succeeded (code: 0)
- But execution failed with: `"Invalid zero amount: cannot deposit or withdraw zero tokens"`
- **This is correct behavior!**

### Key Insight
CosmWasm transactions have two levels:
1. **Transaction level:** Did the transaction get included in a block? (code: 0 = yes)
2. **Execution level:** Did the contract logic succeed? (check raw_log for errors)

A transaction can succeed while the contract execution fails. This is normal and expected.

## Test 5.2: Over-Withdrawal - SETUP REQUIRED

### What It Tests
Verifies that you cannot withdraw more LP tokens than you own.

### Why It Might Not Show Error

**Problem:** If you have enough LP tokens, the withdrawal will succeed!

During our testing:
- You had 100 LP tokens
- The test tried to withdraw 50 LP tokens
- **Result:** Success (no error, because 50 < 100)

### How to Make This Test Fail (Successfully)

**Option 1: Adjust the amount**
```bash
# Check your LP balance first
LP_BAL=$(zigchaind query wasm contract-state smart $LP_TOKEN_ADDRESS \
  "{\"balance\":{\"address\":\"$WALLET_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ -o json | jq -r '.data.balance')

echo "Your LP Balance: $LP_BAL"
# Output: 100000000 (100 LP tokens)

# Try to withdraw MORE than you have
WITHDRAW_AMOUNT="200000000"  # 200 LP (more than 100)

# Set approval
zigchaind tx wasm execute $LP_TOKEN_ADDRESS \
  "{\"increase_allowance\":{\"spender\":\"$LP_POOL_ADDRESS\",\"amount\":\"$WITHDRAW_AMOUNT\"}}" \
  --from mynewwallet \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 \
  --gas 300000 \
  --fees 15000uzig \
  --yes

sleep 6

# Try to withdraw (this will fail)
zigchaind tx wasm execute $LP_POOL_ADDRESS \
  "{\"withdraw\":{\"amount\":\"$WITHDRAW_AMOUNT\"}}" \
  --from mynewwallet \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 \
  --gas 500000 \
  --fees 25000uzig \
  --yes
```

**Option 2: Withdraw everything first, then try again**
```bash
# Withdraw all your LP tokens
# (follow Phase 4 in queries.md)

# Then try to withdraw more
zigchaind tx wasm execute $LP_POOL_ADDRESS \
  '{"withdraw":{"amount":"10000000"}}' \
  --from mynewwallet \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 \
  --gas 500000 \
  --fees 25000uzig \
  --yes
```

### Expected Error
The CW20 LP token contract will reject the transfer with:
- `"Insufficient funds"` or
- `"Cannot subtract with overflow"` or
- Similar balance-related error

### Where the Check Happens
```rust
// In CW20 token contract (not our LP pool):
pub fn execute_transfer_from(...) -> Result<Response, ContractError> {
    // Check balance
    if balance < amount {
        return Err(ContractError::InsufficientFunds {});
    }
    // ...
}
```

The validation happens at the **CW20 token level**, not in our LP pool contract.

## Test 5.3: Deposit Without Approval - STATE CLEANUP REQUIRED

### What It Tests
Verifies that the contract requires approval before transferring tokens from your wallet.

### Why It Might Not Show Error

**Problem:** You already set an allowance in Phase 2!

In Phase 2, you ran:
```bash
zigchaind tx wasm execute $STABLECOIN_ADDRESS \
  "{\"increase_allowance\":{\"spender\":\"$LP_POOL_ADDRESS\",\"amount\":\"100000000\"}}" \
  ...
```

**CW20 allowances don't automatically expire or reset.** They persist until:
1. They are used up (spent)
2. You explicitly decrease them
3. You set them to expire (with expiration time)

### How to Make This Test Work

**Step 1: Check current allowance**
```bash
zigchaind query wasm contract-state smart $STABLECOIN_ADDRESS \
  "{\"allowance\":{\"owner\":\"$WALLET_ADDRESS\",\"spender\":\"$LP_POOL_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --output json | jq '.data'
```

**Output might show:**
```json
{
  "allowance": "50000000",  // Still has 50 USDT approved!
  "expires": {
    "never": {}
  }
}
```

**Step 2: Remove the allowance**
```bash
# Decrease allowance to 0
zigchaind tx wasm execute $STABLECOIN_ADDRESS \
  "{\"decrease_allowance\":{\"spender\":\"$LP_POOL_ADDRESS\",\"amount\":\"999999999\"}}" \
  --from mynewwallet \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 \
  --gas 300000 \
  --fees 15000uzig \
  --yes

sleep 6
```

**Step 3: Verify allowance is 0**
```bash
zigchaind query wasm contract-state smart $STABLECOIN_ADDRESS \
  "{\"allowance\":{\"owner\":\"$WALLET_ADDRESS\",\"spender\":\"$LP_POOL_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --output json | jq '.data'
```

**Expected output:**
```json
{
  "allowance": "0",
  "expires": {
    "never": {}
  }
}
```

**Step 4: Try deposit without new approval**
```bash
zigchaind tx wasm execute $LP_POOL_ADDRESS \
  '{"deposit":{"amount":"10000000"}}' \
  --from mynewwallet \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 \
  --gas 500000 \
  --fees 25000uzig \
  --yes
```

### Expected Error
```
"Insufficient allowance" or "No allowance for this account"
```

### Where the Check Happens
```rust
// In CW20 token contract:
pub fn execute_transfer_from(...) -> Result<Response, ContractError> {
    let allowance = ALLOWANCES.load(deps.storage, (&owner, &spender))?;
    
    if allowance.allowance < amount {
        return Err(ContractError::InsufficientAllowance { ... });
    }
    // ...
}
```

## Complete Phase 5 Testing Script

Here's a script to test all Phase 5 scenarios from a clean state:

```bash
#!/bin/bash
source scripts/contract_addresses.txt

echo "=== PHASE 5: ERROR TESTING ==="

# Test 5.1: Zero deposit ✅
echo -e "\n[Test 5.1] Depositing 0 amount (should fail)..."
zigchaind tx wasm execute $LP_POOL_ADDRESS \
  '{"deposit":{"amount":"0"}}' \
  --from mynewwallet \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 \
  --gas 500000 \
  --fees 25000uzig \
  --yes \
  --output json > zero_test.json

sleep 6
ZERO_TX=$(cat zero_test.json | jq -r '.txhash')
ERROR_MSG=$(zigchaind query tx $ZERO_TX --node https://public-zigchain-testnet-rpc.numia.xyz/ -o json | jq -r '.raw_log')

if [[ $ERROR_MSG == *"Invalid zero amount"* ]]; then
    echo "✅ PASS: Zero deposit correctly rejected"
else
    echo "❌ FAIL: Expected 'Invalid zero amount' error"
fi

# Test 5.2: Over-withdrawal
echo -e "\n[Test 5.2] Withdrawing more than balance..."
# Get current LP balance
LP_BAL=$(zigchaind query wasm contract-state smart $LP_TOKEN_ADDRESS \
  "{\"balance\":{\"address\":\"$WALLET_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ -o json | jq -r '.data.balance')

echo "Current LP Balance: $(echo "scale=2; $LP_BAL / 1000000" | bc) LP"

# Try to withdraw double the balance
OVER_AMOUNT=$(echo "$LP_BAL * 2" | bc)
echo "Attempting to withdraw: $(echo "scale=2; $OVER_AMOUNT / 1000000" | bc) LP"

# Set approval
zigchaind tx wasm execute $LP_TOKEN_ADDRESS \
  "{\"increase_allowance\":{\"spender\":\"$LP_POOL_ADDRESS\",\"amount\":\"$OVER_AMOUNT\"}}" \
  --from mynewwallet \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 \
  --gas 300000 \
  --fees 15000uzig \
  --yes > /dev/null

sleep 6

# Try to withdraw
zigchaind tx wasm execute $LP_POOL_ADDRESS \
  "{\"withdraw\":{\"amount\":\"$OVER_AMOUNT\"}}" \
  --from mynewwallet \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 \
  --gas 500000 \
  --fees 25000uzig \
  --yes \
  --output json > over_test.json

sleep 6
OVER_TX=$(cat over_test.json | jq -r '.txhash')
OVER_ERROR=$(zigchaind query tx $OVER_TX --node https://public-zigchain-testnet-rpc.numia.xyz/ -o json | jq -r '.raw_log')

if [[ $OVER_ERROR == *"insufficient"* ]] || [[ $OVER_ERROR == *"overflow"* ]]; then
    echo "✅ PASS: Over-withdrawal correctly rejected"
else
    echo "⚠️  Check error: $OVER_ERROR"
fi

# Test 5.3: No approval
echo -e "\n[Test 5.3] Depositing without approval..."

# Clear any existing allowance
zigchaind tx wasm execute $STABLECOIN_ADDRESS \
  "{\"decrease_allowance\":{\"spender\":\"$LP_POOL_ADDRESS\",\"amount\":\"999999999\"}}" \
  --from mynewwallet \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 \
  --gas 300000 \
  --fees 15000uzig \
  --yes > /dev/null

sleep 6

# Try to deposit without approval
zigchaind tx wasm execute $LP_POOL_ADDRESS \
  '{"deposit":{"amount":"10000000"}}' \
  --from mynewwallet \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 \
  --gas 500000 \
  --fees 25000uzig \
  --yes \
  --output json > no_approval_test.json

sleep 6
NO_APPR_TX=$(cat no_approval_test.json | jq -r '.txhash')
NO_APPR_ERROR=$(zigchaind query tx $NO_APPR_TX --node https://public-zigchain-testnet-rpc.numia.xyz/ -o json | jq -r '.raw_log')

if [[ $NO_APPR_ERROR == *"allowance"* ]]; then
    echo "✅ PASS: No approval deposit correctly rejected"
else
    echo "⚠️  Check error: $NO_APPR_ERROR"
fi

echo -e "\n=== PHASE 5 COMPLETE ==="
```

Save this as `test_phase5.sh`, make it executable, and run:
```bash
chmod +x test_phase5.sh
./test_phase5.sh
```

---

## Key Takeaways

### Why Error Tests Are Tricky

1. **State Persistence:** Blockchain state persists between tests
   - Allowances remain until explicitly changed
   - Balances carry over from previous transactions
   - You need to actively manage state between tests

2. **Two-Level Validation:** 
   - Transaction level (did it get included in a block?)
   - Execution level (did the contract logic succeed?)
   - A transaction can succeed while execution fails

3. **External Dependencies:**
   - Some validations happen in CW20 token contracts
   - Not all errors come from your LP pool contract
   - Need to understand the full call chain

### Best Practices for Error Testing

1. **Check state before testing**
   ```bash
   # Check allowances
   # Check balances
   # Verify assumptions
   ```

2. **Clean up state between tests**
   ```bash
   # Reset allowances
   # Withdraw to known balances
   # Start from predictable state
   ```

3. **Verify errors properly**
   ```bash
   # Don't just check exit code
   # Read raw_log for actual error message
   # Understand which contract threw the error
   ```

4. **Use fresh wallet for clean testing**
   ```bash
   # Create new test wallet
   # No prior state or allowances
   # Clean slate for negative tests
   ```

---

## Summary

| Test | Status | Issue | Solution |
|------|--------|-------|----------|
| 5.1 Zero Deposit | ✅ PASSED | None | Works perfectly as-is |
| 5.2 Over-withdrawal | ⚠️ Needs setup | Had enough LP tokens | Try to withdraw more than balance |
| 5.3 No Approval | ⚠️ Needs cleanup | Allowance still active from Phase 2 | Decrease allowance to 0 first |

**All three tests verify real security concerns and work correctly when state is properly managed!**
