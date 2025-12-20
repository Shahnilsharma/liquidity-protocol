# Manual Testing Guide - LP Transfer Protocol
## Complete Flow Testing on ZigChain Testnet

This guide provides all queries and commands to manually test the complete liquidity protocol flow on ZigChain testnet.

## Prerequisites

Before starting, ensure you have:
- Deployed contracts successfully (scripts/deploy.sh completed)
- scripts/contract_addresses.txt file exists
- ZigChain CLI installed (zigchaind)
- Wallet with ZIG tokens for gas

**Load your contract addresses:**
```bash
source scripts/contract_addresses.txt
echo "USDT Address: $STABLECOIN_ADDRESS"
echo "LP Token Address: $LP_TOKEN_ADDRESS"
echo "LP Pool Address: $LP_POOL_ADDRESS"
echo "Wallet Address: $WALLET_ADDRESS"
```

## PHASE 1: Initial State Verification

### 1.1 Check Pool Configuration
**Purpose:** Verify the pool is configured correctly with admin and token addresses.

```bash
zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
  '{"config":{}}' \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --output json | jq
```

**Expected Output:**
```json
{
  "admin": "zig1emmn3urg5xzugyah0pcqg96sfw0qwpctmz6cyj",
  "stablecoin_address": "zig1...",
  "lp_token_address": "zig1..."
}
```

---

### 1.2 Check Initial Pool Info
**Purpose:** See total deposits and LP supply (should be 0 initially).

```bash
zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
  '{"pool_info":{}}' \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --output json | jq
```

**Expected Output:**
```json
{
  "total_stablecoin_deposited": "0",
  "total_lp_supply": "0"
}
```

---

### 1.3 Check Your USDT Balance
**Purpose:** Verify you have USDT tokens to deposit (from deployment script mint).

```bash
zigchaind query wasm contract-state smart $STABLECOIN_ADDRESS \
  "{\"balance\":{\"address\":\"$WALLET_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --output json | jq -r '.data.balance'
```

**Expected:** `1000000000` (1000 USDT with 6 decimals)

**Human-readable format:**
```bash
# Divide by 1,000,000 for actual USDT amount
USDT_BAL=$(zigchaind query wasm contract-state smart $STABLECOIN_ADDRESS \
  "{\"balance\":{\"address\":\"$WALLET_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ -o json | jq -r '.data.balance')
echo "USDT Balance: $(echo "scale=2; $USDT_BAL / 1000000" | bc) USDT"
```

---

### 1.4 Check Your LP Token Balance
**Purpose:** Verify you have no LP tokens yet (should be 0).

```bash
zigchaind query wasm contract-state smart $LP_TOKEN_ADDRESS \
  "{\"balance\":{\"address\":\"$WALLET_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --output json | jq -r '.data.balance'
```

**Expected:** `0`

---

### 1.5 Check Your User Info in Pool
**Purpose:** See your deposit history in the pool (should be 0).

```bash
zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
  "{\"user_info\":{\"address\":\"$WALLET_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --output json | jq
```

**Expected Output:**
```json
{
  "address": "zig1emmn3urg5xzugyah0pcqg96sfw0qwpctmz6cyj",
  "deposited_amount": "0"
}
```

## PHASE 2: First Deposit (100 USDT)

### 2.1 Approve LP Pool to Spend Your USDT
**Purpose:** Give permission to the LP pool contract to transfer USDT from your wallet.

```bash
zigchaind tx wasm execute $STABLECOIN_ADDRESS \
  "{\"increase_allowance\":{\"spender\":\"$LP_POOL_ADDRESS\",\"amount\":\"100000000\"}}" \
  --from mynewwallet \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 \
  --gas 300000 \
  --fees 15000uzig \
  --yes
```

**Wait for confirmation:** ~6 seconds
```bash
sleep 6
echo "✓ Approval transaction confirmed"
```

---

### 2.2 Verify Allowance Was Set
**Purpose:** Confirm the pool contract can now spend up to 100 USDT.

```bash
zigchaind query wasm contract-state smart $STABLECOIN_ADDRESS \
  "{\"allowance\":{\"owner\":\"$WALLET_ADDRESS\",\"spender\":\"$LP_POOL_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --output json | jq
```

**Expected Output:**
```json
{
  "data": {
    "allowance": "100000000",
    "expires": {
      "never": {}
    }
  }
}
```

---

### 2.3 Execute Deposit
**Purpose:** Deposit 100 USDT into the pool and receive 100 LP tokens.

```bash
zigchaind tx wasm execute $LP_POOL_ADDRESS \
  '{"deposit":{"amount":"100000000"}}' \
  --from mynewwallet \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 \
  --gas 500000 \
  --fees 25000uzig \
  --yes \
  --output json > deposit_tx.json

# Get transaction hash
DEPOSIT_TX=$(cat deposit_tx.json | jq -r '.txhash')
echo "Deposit TX: $DEPOSIT_TX"
```

**Wait for confirmation:**
```bash
sleep 6
```

---

### 2.4 Check Deposit Transaction Result
**Purpose:** Verify the deposit transaction succeeded.

```bash
zigchaind query tx $DEPOSIT_TX \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --output json | jq '{code: .code, raw_log: .raw_log, events: .events}'
```

**Expected:** `"code": 0` (success)

**View deposit events:**
```bash
zigchaind query tx $DEPOSIT_TX \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --output json | jq -r '.events[] | select(.type=="wasm") | .attributes[] | "\(.key): \(.value)"'
```

**Look for:**
- `method: deposit`
- `user: zig1emmn...`
- `amount: 100000000`

---

### 2.5 Verify Your New USDT Balance
**Purpose:** Confirm 100 USDT was deducted from your wallet.

```bash
USDT_BAL=$(zigchaind query wasm contract-state smart $STABLECOIN_ADDRESS \
  "{\"balance\":{\"address\":\"$WALLET_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ -o json | jq -r '.data.balance')
echo "USDT Balance: $(echo "scale=2; $USDT_BAL / 1000000" | bc) USDT"
```

**Expected:** `900.00 USDT` (1000 - 100)

---

### 2.6 Verify Your New LP Token Balance
**Purpose:** Confirm you received 100 LP tokens (1:1 ratio).

```bash
LP_BAL=$(zigchaind query wasm contract-state smart $LP_TOKEN_ADDRESS \
  "{\"balance\":{\"address\":\"$WALLET_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ -o json | jq -r '.data.balance')
echo "LP Token Balance: $(echo "scale=2; $LP_BAL / 1000000" | bc) LP"
```

**Expected:** `100.00 LP`

---

### 2.7 Verify Pool Total Deposits
**Purpose:** Check the pool now holds 100 USDT and has 100 LP supply.

```bash
zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
  '{"pool_info":{}}' \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --output json | jq
```

**Expected Output:**
```json
{
  "total_stablecoin_deposited": "100000000",
  "total_lp_supply": "100000000"
}
```

---

### 2.8 Verify Your User Info
**Purpose:** Check your personal deposit record in the pool.

```bash
zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
  "{\"user_info\":{\"address\":\"$WALLET_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --output json | jq
```

**Expected Output:**
```json
{
  "address": "zig1emmn3urg5xzugyah0pcqg96sfw0qwpctmz6cyj",
  "deposited_amount": "100000000"
}
```

## PHASE 3: Partial Withdrawal (50 LP)

### 3.1 Approve LP Pool to Burn Your LP Tokens
**Purpose:** Give permission for the pool to burn your LP tokens during withdrawal.

```bash
zigchaind tx wasm execute $LP_TOKEN_ADDRESS \
  "{\"increase_allowance\":{\"spender\":\"$LP_POOL_ADDRESS\",\"amount\":\"50000000\"}}" \
  --from mynewwallet \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 \
  --gas 300000 \
  --fees 15000uzig \
  --yes
```

**Wait for confirmation:**
```bash
sleep 6
echo "✓ LP token allowance set"
```

---

### 3.2 Verify LP Token Allowance
**Purpose:** Confirm the pool can burn up to 50 LP tokens.

```bash
zigchaind query wasm contract-state smart $LP_TOKEN_ADDRESS \
  "{\"allowance\":{\"owner\":\"$WALLET_ADDRESS\",\"spender\":\"$LP_POOL_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --output json | jq
```

**Expected Output:**
```json
{
  "data": {
    "allowance": "50000000",
    "expires": {
      "never": {}
    }
  }
}
```

---

### 3.3 Execute Withdrawal
**Purpose:** Withdraw 50 USDT by burning 50 LP tokens.

```bash
zigchaind tx wasm execute $LP_POOL_ADDRESS \
  '{"withdraw":{"amount":"50000000"}}' \
  --from mynewwallet \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 \
  --gas 500000 \
  --fees 25000uzig \
  --yes \
  --output json > withdraw_tx.json

WITHDRAW_TX=$(cat withdraw_tx.json | jq -r '.txhash')
echo "Withdraw TX: $WITHDRAW_TX"
```

**Wait for confirmation:**
```bash
sleep 6
```

---

### 3.4 Check Withdrawal Transaction Result
**Purpose:** Verify the withdrawal succeeded.

```bash
zigchaind query tx $WITHDRAW_TX \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --output json | jq '{code: .code, raw_log: .raw_log}'
```

**Expected:** `"code": 0`

**View withdrawal events:**
```bash
zigchaind query tx $WITHDRAW_TX \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --output json | jq -r '.events[] | select(.type=="wasm") | .attributes[] | "\(.key): \(.value)"'
```

**Look for:**
- `method: withdraw`
- `user: zig1emmn...`
- `amount: 50000000`

---

### 3.5 Verify Your USDT Balance After Withdrawal
**Purpose:** Confirm 50 USDT was returned to your wallet.

```bash
USDT_BAL=$(zigchaind query wasm contract-state smart $STABLECOIN_ADDRESS \
  "{\"balance\":{\"address\":\"$WALLET_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ -o json | jq -r '.data.balance')
echo "USDT Balance: $(echo "scale=2; $USDT_BAL / 1000000" | bc) USDT"
```

**Expected:** `950.00 USDT` (900 + 50)

---

### 3.6 Verify Your LP Token Balance After Withdrawal
**Purpose:** Confirm 50 LP tokens were burned.

```bash
LP_BAL=$(zigchaind query wasm contract-state smart $LP_TOKEN_ADDRESS \
  "{\"balance\":{\"address\":\"$WALLET_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ -o json | jq -r '.data.balance')
echo "LP Token Balance: $(echo "scale=2; $LP_BAL / 1000000" | bc) LP"
```

**Expected:** `50.00 LP` (100 - 50)

---

### 3.7 Verify Pool State After Withdrawal
**Purpose:** Check pool now has 50 USDT and 50 LP supply.

```bash
zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
  '{"pool_info":{}}' \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --output json | jq
```

**Expected Output:**
```json
{
  "total_stablecoin_deposited": "50000000",
  "total_lp_supply": "50000000"
}
```

---

### 3.8 Verify Your Updated User Info
**Purpose:** Check your deposit record decreased by 50 USDT.

```bash
zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
  "{\"user_info\":{\"address\":\"$WALLET_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --output json | jq
```

**Expected Output:**
```json
{
  "address": "zig1emmn3urg5xzugyah0pcqg96sfw0qwpctmz6cyj",
  "deposited_amount": "50000000"
}
```

## PHASE 4: Complete Withdrawal (Remaining 50 LP)

### 4.1 Approve Remaining LP Tokens
```bash
zigchaind tx wasm execute $LP_TOKEN_ADDRESS \
  "{\"increase_allowance\":{\"spender\":\"$LP_POOL_ADDRESS\",\"amount\":\"50000000\"}}" \
  --from mynewwallet \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 \
  --gas 300000 \
  --fees 15000uzig \
  --yes
```

```bash
sleep 6
```

---

### 4.2 Withdraw Remaining Amount
```bash
zigchaind tx wasm execute $LP_POOL_ADDRESS \
  '{"withdraw":{"amount":"50000000"}}' \
  --from mynewwallet \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 \
  --gas 500000 \
  --fees 25000uzig \
  --yes \
  --output json > final_withdraw_tx.json

FINAL_TX=$(cat final_withdraw_tx.json | jq -r '.txhash')
sleep 6
```

---

### 4.3 Verify Final Balances
**Your USDT should be back to 1000:**
```bash
USDT_BAL=$(zigchaind query wasm contract-state smart $STABLECOIN_ADDRESS \
  "{\"balance\":{\"address\":\"$WALLET_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ -o json | jq -r '.data.balance')
echo "Final USDT Balance: $(echo "scale=2; $USDT_BAL / 1000000" | bc) USDT"
```

**Expected:** `1000.00 USDT`

**Your LP tokens should be 0:**
```bash
LP_BAL=$(zigchaind query wasm contract-state smart $LP_TOKEN_ADDRESS \
  "{\"balance\":{\"address\":\"$WALLET_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ -o json | jq -r '.data.balance')
echo "Final LP Balance: $(echo "scale=2; $LP_BAL / 1000000" | bc) LP"
```

**Expected:** `0.00 LP`

---

### 4.4 Verify Pool is Empty
```bash
zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
  '{"pool_info":{}}' \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --output json | jq
```

**Expected:**
```json
{
  "total_stablecoin_deposited": "0",
  "total_lp_supply": "0"
}
```

## PHASE 5: Edge Cases & Error Testing

### 5.1 Try Depositing 0 Amount (Should Fail)
**Purpose:** Verify the contract rejects zero deposits.

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

**Expected Result:** Transaction succeeds with code 0, but execution fails with error message.

**To check the error:**
```bash
# Get the transaction hash from the output above, then:
zigchaind query tx <TX_HASH> \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  -o json | jq -r '.raw_log'
```

**Expected Error Message:** `"Invalid zero amount: cannot deposit or withdraw zero tokens"`

**Status:** ✅ **VERIFIED** - This test passed successfully. The contract correctly rejects zero amount deposits.

---

### 5.2 Try Withdrawing More Than Balance (Should Fail)
**Purpose:** Verify the contract prevents over-withdrawal.

**⚠️ Important:** This test requires you to have fewer LP tokens than you're trying to withdraw.

```bash
# First, check your current LP balance
LP_BAL=$(zigchaind query wasm contract-state smart $LP_TOKEN_ADDRESS \
  "{\"balance\":{\"address\":\"$WALLET_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ -o json | jq -r '.data.balance')
echo "Your LP Balance: $LP_BAL"

# Try to withdraw more than you have (e.g., if you have 100, try 200)
ATTEMPT_AMOUNT="200000000"  # 200 LP tokens

# First approve the amount
zigchaind tx wasm execute $LP_TOKEN_ADDRESS \
  "{\"increase_allowance\":{\"spender\":\"$LP_POOL_ADDRESS\",\"amount\":\"$ATTEMPT_AMOUNT\"}}" \
  --from mynewwallet \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 \
  --gas 300000 \
  --fees 15000uzig \
  --yes

sleep 6

# Try to withdraw (this should fail)
zigchaind tx wasm execute $LP_POOL_ADDRESS \
  "{\"withdraw\":{\"amount\":\"$ATTEMPT_AMOUNT\"}}" \
  --from mynewwallet \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 \
  --gas 500000 \
  --fees 25000uzig \
  --yes
```

**Expected Error:** Transaction will fail with insufficient funds error from CW20 transfer.

**Why This Test Might Not Show Error:**
- If you have enough LP tokens, the withdrawal will succeed
- The error only occurs when you try to transfer more LP tokens than you own
- The CW20 token contract checks balance before allowing transfer

**To Make This Test Fail Successfully:**
1. Ensure you have less than 200 LP tokens
2. Or adjust `ATTEMPT_AMOUNT` to be more than your actual balance

---

### 5.3 Try Depositing Without Prior Approval (Should Fail)
**Purpose:** Verify the contract requires approval before accepting deposits.

**⚠️ Important:** Make sure you don't have an existing allowance set.

```bash
# First, check current allowance
zigchaind query wasm contract-state smart $STABLECOIN_ADDRESS \
  "{\"allowance\":{\"owner\":\"$WALLET_ADDRESS\",\"spender\":\"$LP_POOL_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --output json | jq '.data'

# If allowance is > 0, you need to decrease it first:
# zigchaind tx wasm execute $STABLECOIN_ADDRESS \
#   "{\"decrease_allowance\":{\"spender\":\"$LP_POOL_ADDRESS\",\"amount\":\"999999999\"}}" \
#   --from mynewwallet --node https://public-zigchain-testnet-rpc.numia.xyz/ \
#   --chain-id zig-test-2 --gas 300000 --fees 15000uzig --yes

# Now try to deposit WITHOUT setting approval
zigchaind tx wasm execute $LP_POOL_ADDRESS \
  '{"deposit":{"amount":"10000000"}}' \
  --from mynewwallet \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 \
  --gas 500000 \
  --fees 25000uzig \
  --yes
```

**Expected Error:** `"Insufficient allowance"` or `"No allowance for this account"`

**Why This Test Might Not Work:**
- If you previously set an allowance in Phase 2, it might still be active
- CW20 allowances don't expire unless explicitly removed
- You need to decrease/remove the allowance first to test this properly

**How to Make This Test Work:**
1. Check existing allowance with the query above
2. If allowance exists, use `decrease_allowance` to remove it
3. Then try the deposit without approval

---

## 📌 Phase 5 Summary

**Test Results:**
- ✅ **Test 5.1 (Zero Deposit):** PASSED - Contract correctly rejects with "Invalid zero amount"
- ⚠️ **Test 5.2 (Over-withdrawal):** Requires manual setup (need insufficient LP balance)
- ⚠️ **Test 5.3 (No Approval):** Requires removing existing allowances first

**Key Learnings:**
1. **Zero amount validation works perfectly** - The contract has proper input validation
2. **Balance checks happen at CW20 level** - Over-withdrawal is prevented by the token contract
3. **Allowances persist** - Once set, they remain until explicitly decreased
4. **Testing negative cases requires careful state setup** - You need to ensure preconditions are met

**Recommended Approach for Full Error Testing:**
1. Test on a fresh wallet with no prior allowances
2. Test over-withdrawal immediately after a small deposit
3. Use zero amount tests as quick validation checks

## PHASE 6: Summary Queries

### 6.1 Get All Contract Addresses
```bash
echo "=== Contract Addresses ==="
echo "USDT (Stablecoin): $STABLECOIN_ADDRESS"
echo "LP Token: $LP_TOKEN_ADDRESS"
echo "LP Pool: $LP_POOL_ADDRESS"
echo "Your Wallet: $WALLET_ADDRESS"
```

---

### 6.2 Complete Balance Summary
```bash
echo -e "\n=== Complete Balance Summary ==="

# USDT
USDT_BAL=$(zigchaind query wasm contract-state smart $STABLECOIN_ADDRESS \
  "{\"balance\":{\"address\":\"$WALLET_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ -o json | jq -r '.data.balance')
echo "Your USDT: $(echo "scale=2; $USDT_BAL / 1000000" | bc)"

# LP Tokens
LP_BAL=$(zigchaind query wasm contract-state smart $LP_TOKEN_ADDRESS \
  "{\"balance\":{\"address\":\"$WALLET_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ -o json | jq -r '.data.balance')
echo "Your LP Tokens: $(echo "scale=2; $LP_BAL / 1000000" | bc)"

# Pool Stats
POOL_INFO=$(zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
  '{"pool_info":{}}' --node https://public-zigchain-testnet-rpc.numia.xyz/ -o json | jq '.data')
TOTAL_USDT=$(echo $POOL_INFO | jq -r '.total_stablecoin_deposited')
TOTAL_LP=$(echo $POOL_INFO | jq -r '.total_lp_supply')

echo "Pool Total USDT: $(echo "scale=2; $TOTAL_USDT / 1000000" | bc)"
echo "Pool Total LP: $(echo "scale=2; $TOTAL_LP / 1000000" | bc)"

# Your deposit record
USER_DEPOSIT=$(zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
  "{\"user_info\":{\"address\":\"$WALLET_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ -o json | jq -r '.data.deposited_amount')
echo "Your Recorded Deposit: $(echo "scale=2; $USER_DEPOSIT / 1000000" | bc)"
```

---

### 6.3 View All Token Info
**USDT Token Info:**
```bash
zigchaind query wasm contract-state smart $STABLECOIN_ADDRESS \
  '{"token_info":{}}' \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --output json | jq
```

**LP Token Info:**
```bash
zigchaind query wasm contract-state smart $LP_TOKEN_ADDRESS \
  '{"token_info":{}}' \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --output json | jq
```

## Quick Test Script

Copy this entire block to test the full flow quickly:

```bash
#!/bin/bash
source scripts/contract_addresses.txt

echo "=== FULL FLOW TEST ==="

# Initial state
echo -e "\n1. Initial Balances:"
USDT=$(zigchaind query wasm contract-state smart $STABLECOIN_ADDRESS \
  "{\"balance\":{\"address\":\"$WALLET_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ -o json | jq -r '.data.balance')
echo "USDT: $(echo "scale=2; $USDT / 1000000" | bc)"

# Deposit 100
echo -e "\n2. Depositing 100 USDT..."
zigchaind tx wasm execute $STABLECOIN_ADDRESS \
  "{\"increase_allowance\":{\"spender\":\"$LP_POOL_ADDRESS\",\"amount\":\"100000000\"}}" \
  --from mynewwallet --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 --gas 300000 --fees 15000uzig --yes > /dev/null
sleep 6

zigchaind tx wasm execute $LP_POOL_ADDRESS \
  '{"deposit":{"amount":"100000000"}}' \
  --from mynewwallet --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 --gas 500000 --fees 25000uzig --yes > /dev/null
sleep 6

LP=$(zigchaind query wasm contract-state smart $LP_TOKEN_ADDRESS \
  "{\"balance\":{\"address\":\"$WALLET_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ -o json | jq -r '.data.balance')
echo "LP Tokens Received: $(echo "scale=2; $LP / 1000000" | bc)"

# Withdraw 100
echo -e "\n3. Withdrawing 100 LP..."
zigchaind tx wasm execute $LP_TOKEN_ADDRESS \
  "{\"increase_allowance\":{\"spender\":\"$LP_POOL_ADDRESS\",\"amount\":\"100000000\"}}" \
  --from mynewwallet --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 --gas 300000 --fees 15000uzig --yes > /dev/null
sleep 6

zigchaind tx wasm execute $LP_POOL_ADDRESS \
  '{"withdraw":{"amount":"100000000"}}' \
  --from mynewwallet --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 --gas 500000 --fees 25000uzig --yes > /dev/null
sleep 6

USDT_FINAL=$(zigchaind query wasm contract-state smart $STABLECOIN_ADDRESS \
  "{\"balance\":{\"address\":\"$WALLET_ADDRESS\"}}" \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ -o json | jq -r '.data.balance')
echo "Final USDT: $(echo "scale=2; $USDT_FINAL / 1000000" | bc)"

echo -e "\n✓ Test Complete!"
```

## Notes

- **Decimal Conversion:** All amounts use 6 decimals (1 USDT = 1,000,000 micro-USDT)
- **Gas Estimates:** 
  - Approvals: ~300,000 gas
  - Deposits/Withdrawals: ~500,000 gas
- **Confirmation Time:** ~6 seconds per transaction on ZigChain testnet
- **Transaction Fees:** ~15,000-25,000 uzig per transaction

## Troubleshooting

**If transactions fail:**
1. Check wallet has ZIG for gas fees
2. Verify contract addresses are correct
3. Check allowances are set before deposits/withdrawals
4. Ensure sufficient balance before operations

**To view detailed error logs:**
```bash
zigchaind query tx <TX_HASH> --node https://public-zigchain-testnet-rpc.numia.xyz/ -o json | jq -r '.raw_log'
```