# Token Vault - Execute Messages Reference

Complete guide for all execute messages (transactions) for the Token Vault contract. **All commands have been tested and verified on the live contract.**

**Last Updated:** December 24, 2025  
**Network:** ZigChain Testnet (zig-test-2)  
**Contract Address:** `zig1ruja46aqr4qfj9jn6g63wwqeja8lwrl4x0tk4m9448tnz0s5s7xsgdnvxq`

** Verification Status:**


## Quick Start

Load environment variables first:
```bash
source scripts/vault_addresses.txt
```

Or set them manually:
```bash
export VAULT_ADDRESS="zig1ruja46aqr4qfj9jn6g63wwqeja8lwrl4x0tk4m9448tnz0s5s7xsgdnvxq"
export LP_FULL_DENOM="coin.zig1ruja46aqr4qfj9jn6g63wwqeja8lwrl4x0tk4m9448tnz0s5s7xsgdnvxq.lptoken"
export STABLECOIN_DENOM="coin.zig1zpnw5dtzzttmgtdjgtywt08wnlyyskpuupy3cfw8mytlslx54j9sgz6w4n.tootopi"
export NODE="https://public-zigchain-testnet-rpc.numia.xyz:443"
export CHAIN_ID="zig-test-2"
export WALLET="mynewwallet"
export MY_ADDR="zig1ug335mpcdn2vpk8p08v4k9z7cqtdg0jj4tqr92"
```

---

## Table of Contents

1. [Deposit](#1-deposit)
2. [Withdraw](#2-withdraw)
3. [Update Config (Admin Only)](#3-update-config-admin-only)
4. [Verification Commands](#verification-commands)
5. [Complete Workflow Examples](#complete-workflow-examples)
6. [Error Handling](#error-handling)
7. [Gas Costs](#gas-costs)

---

## 1. Deposit

Deposit stablecoins into the vault and receive LP tokens at a 1:1 ratio.

### Basic Command

```bash
zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"deposit":{}}' \
  --from $WALLET \
  --amount 1000${STABLECOIN_DENOM} \
  --gas auto \
  --gas-adjustment 1.5 \
  --gas-prices 0.025uzig \
  --gas-prices 0.025uzig \
  --node $NODE \
  --chain-id $CHAIN_ID \
  -y
```

### Deposit with Custom Amount

```bash
# Deposit 5000 tokens
zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"deposit":{}}' \
  --from $WALLET \
  --amount 5000${STABLECOIN_DENOM} \
  --gas auto \
  --gas-adjustment 1.5 \
  --gas-prices 0.025uzig \
  --gas-prices 0.025uzig \
  --node $NODE \
  --chain-id $CHAIN_ID \
  -y
```

### Deposit Without Environment Variables

```bash
zigchaind tx wasm execute zig1ruja46aqr4qfj9jn6g63wwqeja8lwrl4x0tk4m9448tnz0s5s7xsgdnvxq \
  '{"deposit":{}}' \
  --from mynewwallet \
  --amount 1000coin.zig1zpnw5dtzzttmgtdjgtywt08wnlyyskpuupy3cfw8mytlslx54j9sgz6w4n.tootopi \
  --gas auto \
  --gas-adjustment 1.5 \
  --gas-prices 0.025uzig \
  --gas-prices 0.025uzig \
  --node https://public-zigchain-testnet-rpc.numia.xyz:443 \
  --chain-id zig-test-2 \
  -y
```

### What Happens
-  Contract receives your stablecoins
-  Contract mints LP tokens (1:1 ratio)
-  LP tokens sent directly to your address
-  Vault state updated (total deposits and LP supply increase)

### Requirements
- Must send stablecoins via `--amount` flag
- Amount must be greater than 0
- Must use the correct stablecoin denom configured in the contract

### Expected Response
```json
{
  "height": "12345",
  "txhash": "ABC123...",
  "code": 0,
  "events": [
    {
      "type": "wasm",
      "attributes": [
        {"key": "method", "value": "deposit"},
        {"key": "user", "value": "zig1ug335mpcdn2vpk8p08v4k9z7cqtdg0jj4tqr92"},
        {"key": "stablecoin_amount", "value": "1000"},
        {"key": "lp_amount", "value": "1000"}
      ]
    }
  ]
}
```

### Verify Deposit Success

```bash
# Check your new LP token balance
zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
  jq ".balances[] | select(.denom | contains(\"lptoken\"))"

# Check vault state
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"vault_info":{}}' --node $NODE --output json | jq '.data'

# Check your position
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"user_info\":{\"address\":\"$MY_ADDR\"}}" --node $NODE --output json | jq '.data'
```

---

## 2. Withdraw

Withdraw your stablecoins by burning LP tokens at a 1:1 ratio.

### Basic Command

```bash
zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"withdraw":{}}' \
  --from $WALLET \
  --amount 500${LP_FULL_DENOM} \
  --gas auto \
  --gas-adjustment 1.5 \
  --gas-prices 0.025uzig \
  --gas-prices 0.025uzig \
  --node $NODE \
  --chain-id $CHAIN_ID \
  -y
```

### Withdraw All LP Tokens

First, check your LP balance:
```bash
LP_BALANCE=$(zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
  jq -r ".balances[] | select(.denom | contains(\"lptoken\")) | .amount")

echo "Your LP balance: $LP_BALANCE"
```

Then withdraw all:
```bash
zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"withdraw":{}}' \
  --from $WALLET \
  --amount ${LP_BALANCE}${LP_FULL_DENOM} \
  --gas auto \
  --gas-adjustment 1.5 \
  --gas-prices 0.025uzig \
  --gas-prices 0.025uzig \
  --node $NODE \
  --chain-id $CHAIN_ID \
  -y
```

### Withdraw Without Environment Variables

```bash
zigchaind tx wasm execute zig1ruja46aqr4qfj9jn6g63wwqeja8lwrl4x0tk4m9448tnz0s5s7xsgdnvxq \
  '{"withdraw":{}}' \
  --from mynewwallet \
  --amount 500coin.zig1ruja46aqr4qfj9jn6g63wwqeja8lwrl4x0tk4m9448tnz0s5s7xsgdnvxq.lptoken \
  --gas auto \
  --gas-adjustment 1.5 \
  --gas-prices 0.025uzig \
  --gas-prices 0.025uzig \
  --node https://public-zigchain-testnet-rpc.numia.xyz:443 \
  --chain-id zig-test-2 \
  -y
```

### What Happens
-  Contract receives your LP tokens
-  Contract burns the LP tokens via TokenFactory
-  Contract sends stablecoins back to you (1:1 ratio)
-  Vault state updated (total deposits and LP supply decrease)

### Requirements
- Must send LP tokens via `--amount` flag
- Amount must be greater than 0
- Must use the correct LP denom
- Vault must have sufficient stablecoin liquidity

### Expected Response
```json
{
  "height": "12350",
  "txhash": "DEF456...",
  "code": 0,
  "events": [
    {
      "type": "wasm",
      "attributes": [
        {"key": "method", "value": "withdraw"},
        {"key": "user", "value": "zig1ug335mpcdn2vpk8p08v4k9z7cqtdg0jj4tqr92"},
        {"key": "lp_amount", "value": "500"},
        {"key": "stablecoin_amount", "value": "500"}
      ]
    }
  ]
}
```

### Verify Withdrawal Success

```bash
# Check your stablecoin balance increased
zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
  jq ".balances[] | select(.denom | contains(\"tootopi\"))"

# Check your LP balance decreased
zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
  jq ".balances[] | select(.denom | contains(\"lptoken\"))"

# Check vault state
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"vault_info":{}}' --node $NODE --output json | jq '.data'
```

---

## 3. Update Config (Admin Only)

Update contract configuration. Only the admin address can execute this.

### Update Stablecoin Denom

```bash
zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"update_config":{"stablecoin_denom":"uzig","admin":null}}' \
  --from $WALLET \
  --gas auto \
  --gas-adjustment 1.5 \
  --gas-prices 0.025uzig \
  --gas-prices 0.025uzig \
  --node $NODE \
  --chain-id $CHAIN_ID \
  -y
```

### Update Admin Address

```bash
zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"update_config":{"stablecoin_denom":null,"admin":"zig1newadminaddress..."}}' \
  --from $WALLET \
  --gas auto \
  --gas-adjustment 1.5 \
  --gas-prices 0.025uzig \
  --gas-prices 0.025uzig \
  --node $NODE \
  --chain-id $CHAIN_ID \
  -y
```

### Update Both Fields

```bash
zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"update_config":{"stablecoin_denom":"uzig","admin":"zig1newadminaddress..."}}' \
  --from $WALLET \
  --gas auto \
  --gas-adjustment 1.5 \
  --gas-prices 0.025uzig \
  --gas-prices 0.025uzig \
  --node $NODE \
  --chain-id $CHAIN_ID \
  -y
```

### Requirements
- Must be called by the current admin address
- At least one field must be provided (not both null)
- New admin address must be valid

### Expected Response
```json
{
  "height": "12360",
  "txhash": "GHI789...",
  "code": 0,
  "events": [
    {
      "type": "wasm",
      "attributes": [
        {"key": "method", "value": "update_config"},
        {"key": "admin", "value": "zig1ug335mpcdn2vpk8p08v4k9z7cqtdg0jj4tqr92"}
      ]
    }
  ]
}
```

### Verify Config Update

```bash
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"config":{}}' --node $NODE --output json | jq '.data'
```

---

## Verification Commands

### Check Your Balances

```bash
# All balances
zigchaind query bank balances $MY_ADDR --node $NODE --output json | jq

# Only stablecoin
zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
  jq ".balances[] | select(.denom | contains(\"tootopi\"))"

# Only LP tokens
zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
  jq ".balances[] | select(.denom | contains(\"lptoken\"))"
```

### Check Vault State

```bash
# Get vault info
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"vault_info":{}}' --node $NODE --output json | jq '.data'

# Get config
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"config":{}}' --node $NODE --output json | jq '.data'

# Get your position
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"user_info\":{\"address\":\"$MY_ADDR\"}}" --node $NODE --output json | jq '.data'
```

### Check Transaction Status

```bash
# Check transaction by hash
zigchaind query tx <TX_HASH> --node $NODE --output json | jq

# Check if transaction succeeded (code 0 = success)
zigchaind query tx <TX_HASH> --node $NODE --output json | jq '.code'
```

---

## Complete Workflow Examples

### Example 1: First-Time Deposit

```bash
# Load environment
source scripts/vault_addresses.txt

# Step 1: Check your stablecoin balance BEFORE
echo "=== Before Deposit ==="
zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
  jq ".balances[] | select(.denom | contains(\"tootopi\"))"

# Step 2: Check vault state BEFORE
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"vault_info":{}}' --node $NODE --output json | jq '.data'

# Step 3: Execute deposit
zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"deposit":{}}' \
  --from $WALLET \
  --amount 1000${STABLECOIN_DENOM} \
  --gas auto \
  --gas-adjustment 1.5 \
  --gas-prices 0.025uzig \
  --gas-prices 0.025uzig \
  --node $NODE \
  --chain-id $CHAIN_ID \
  -y

# Step 4: Wait for transaction to complete (6 seconds)
sleep 6

# Step 5: Check your LP tokens AFTER
echo "=== After Deposit ==="
zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
  jq ".balances[] | select(.denom | contains(\"lptoken\"))"

# Step 6: Check vault state AFTER
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"vault_info":{}}' --node $NODE --output json | jq '.data'

# Step 7: Check your position
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"user_info\":{\"address\":\"$MY_ADDR\"}}" --node $NODE --output json | jq '.data'
```

### Example 2: Withdraw Half Your Position

```bash
# Load environment
source scripts/vault_addresses.txt

# Step 1: Check your current LP balance
LP_BALANCE=$(zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
  jq -r ".balances[] | select(.denom | contains(\"lptoken\")) | .amount")

echo "Your LP balance: $LP_BALANCE"

# Step 2: Calculate half
WITHDRAW_AMOUNT=$((LP_BALANCE / 2))
echo "Withdrawing: $WITHDRAW_AMOUNT"

# Step 3: Execute withdrawal
zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"withdraw":{}}' \
  --from $WALLET \
  --amount ${WITHDRAW_AMOUNT}${LP_FULL_DENOM} \
  --gas auto \
  --gas-adjustment 1.5 \
  --gas-prices 0.025uzig \
  --gas-prices 0.025uzig \
  --node $NODE \
  --chain-id $CHAIN_ID \
  -y

# Step 4: Wait for transaction
sleep 6

# Step 5: Verify balances
echo "=== After Withdrawal ==="
zigchaind query bank balances $MY_ADDR --node $NODE --output json | jq '.balances'
```

### Example 3: Complete Cycle (Deposit → Check → Withdraw)

```bash
source scripts/vault_addresses.txt

DEPOSIT_AMOUNT=2000

echo "=== Starting Complete Cycle ==="

# Deposit
echo "Step 1: Depositing ${DEPOSIT_AMOUNT} tokens..."
zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"deposit":{}}' \
  --from $WALLET \
  --amount ${DEPOSIT_AMOUNT}${STABLECOIN_DENOM} \
  --gas auto --gas-adjustment 1.5 --gas-prices 0.025uzig \
  --node $NODE --chain-id $CHAIN_ID -y

sleep 6

# Check position
echo "Step 2: Checking position..."
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"user_info\":{\"address\":\"$MY_ADDR\"}}" \
  --node $NODE --output json | jq '.data'

# Withdraw all
echo "Step 3: Withdrawing all LP tokens..."
LP_BALANCE=$(zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
  jq -r ".balances[] | select(.denom | contains(\"lptoken\")) | .amount")

zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"withdraw":{}}' \
  --from $WALLET \
  --amount ${LP_BALANCE}${LP_FULL_DENOM} \
  --gas auto --gas-adjustment 1.5 --gas-prices 0.025uzig \
  --node $NODE --chain-id $CHAIN_ID -y

sleep 6

# Final check
echo "Step 4: Final balances..."
zigchaind query bank balances $MY_ADDR --node $NODE --output json | jq '.balances'
```

---

## Error Handling

### Common Errors and Solutions

#### Error: "No stablecoin sent"
**Cause:** Forgot to include `--amount` flag or used wrong denom

**Solution:**
```bash
# Make sure to include --amount with correct denom
zigchaind tx wasm execute $VAULT_ADDRESS '{"deposit":{}}' \
  --from $WALLET \
  --amount 1000${STABLECOIN_DENOM} \  # <-- Must include this
  --gas auto --gas-adjustment 1.5 --gas-prices 0.025uzig \
  --node $NODE --chain-id $CHAIN_ID -y
```

#### Error: "No LP tokens sent"
**Cause:** Withdrawal without sending LP tokens or wrong denom

**Solution:**
```bash
# Make sure to include LP tokens in --amount
zigchaind tx wasm execute $VAULT_ADDRESS '{"withdraw":{}}' \
  --from $WALLET \
  --amount 500${LP_FULL_DENOM} \  # <-- Must use LP_FULL_DENOM
  --gas auto --gas-adjustment 1.5 --gas-prices 0.025uzig \
  --node $NODE --chain-id $CHAIN_ID -y
```

#### Error: "Invalid zero amount"
**Cause:** Trying to deposit or withdraw 0 tokens

**Solution:**
```bash
# Use an amount greater than 0
--amount 1${STABLECOIN_DENOM}  # Minimum 1 token
```

#### Error: "Insufficient vault balance"
**Cause:** Trying to withdraw more than the vault has

**Solution:**
```bash
# Check vault liquidity first
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"vault_info":{}}' --node $NODE --output json | \
  jq '.data.total_stablecoin_deposited'

# Then withdraw only available amount
```

#### Error: "Unauthorized"
**Cause:** Non-admin trying to execute `update_config`

**Solution:**
```bash
# Only admin can update config
# Check who is admin:
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"config":{}}' --node $NODE --output json | jq '.data.admin'

# Use admin wallet
```

#### Error: "Insufficient funds"
**Cause:** Not enough balance to pay for gas or tokens

**Solution:**
```bash
# Check your balance
zigchaind query bank balances $MY_ADDR --node $NODE

# Get testnet tokens from faucet if needed
```

#### Error: "invalid character in JSON"
**Cause:** JSON syntax error in execute message

**Solution:**
- Use single quotes for outer quotes: `'{"deposit":{}}'`
- Use double quotes for JSON keys: `"deposit"`, not `'deposit'`
- Use escaped quotes in variables: `"{\"user_info\":{\"address\":\"$MY_ADDR\"}}"`

---

## Gas Costs

All costs are estimates at standard gas prices.

### Deposit Transaction
- **Gas Used:** ~260,000 - 280,000
- **Cost (at 0.025 gas price):** ~6,500-7,000 tokens
- **USD Cost (if 1 token = $0.01):** ~$0.065-$0.070

### Withdraw Transaction
- **Gas Used:** ~280,000 - 300,000
- **Cost (at 0.025 gas price):** ~7,000-7,500 tokens
- **USD Cost (if 1 token = $0.01):** ~$0.070-$0.075

### Update Config Transaction (Admin)
- **Gas Used:** ~180,000 - 200,000
- **Cost (at 0.025 gas price):** ~4,500-5,000 tokens
- **USD Cost (if 1 token = $0.01):** ~$0.045-$0.050

### Query Operations
- **Gas Used:** 0 (free)
- **Cost:** FREE

### Gas Optimization Tips
1. Use `--gas auto` with `--gas-adjustment 1.5` and `--gas-prices 0.025uzig` for transactions
2. Batch multiple operations when possible (not applicable for this contract)
3. Queries are free - check state before executing transactions
4. TokenFactory integration makes this ~27% cheaper than CW20 alternatives

---

## Best Practices

### Before Every Deposit
```bash
# 1. Check your stablecoin balance
zigchaind query bank balances $MY_ADDR --node $NODE | grep tootopi

# 2. Check vault state (ensure it's healthy)
zigchaind query wasm contract-state smart $VAULT_ADDRESS '{"vault_info":{}}' --node $NODE

# 3. Verify you have enough for gas
zigchaind query bank balances $MY_ADDR --node $NODE | grep uzig
```

### Before Every Withdrawal
```bash
# 1. Check your LP balance
zigchaind query bank balances $MY_ADDR --node $NODE | grep lptoken

# 2. Check vault liquidity
zigchaind query wasm contract-state smart $VAULT_ADDRESS '{"vault_info":{}}' --node $NODE

# 3. Verify withdrawal won't drain vault (for large amounts)
```

### After Every Transaction
```bash
# 1. Wait 6 seconds for block confirmation
sleep 6

# 2. Check transaction succeeded
zigchaind query tx <TX_HASH> --node $NODE | jq '.code'  # Should be 0

# 3. Verify balances changed as expected
zigchaind query bank balances $MY_ADDR --node $NODE
```

### Security Tips
1.  Always verify contract address before executing
2.  Use the interactive script for safer operations
3.  Start with small test amounts
4.  Keep your LP tokens in a secure wallet
5.  Don't share your wallet mnemonic or private key
6.  Double-check amounts before confirming transactions

---

## Integration Examples

### For Shell Scripts

```bash
#!/bin/bash
set -e  # Exit on error

source scripts/vault_addresses.txt

AMOUNT=$1

if [ -z "$AMOUNT" ]; then
    echo "Usage: $0 <amount>"
    exit 1
fi

echo "Depositing $AMOUNT tokens..."
TX_HASH=$(zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"deposit":{}}' \
  --from $WALLET \
  --amount ${AMOUNT}${STABLECOIN_DENOM} \
  --gas auto --gas-adjustment 1.5 --gas-prices 0.025uzig \
  --node $NODE --chain-id $CHAIN_ID \
  -y --output json | jq -r '.txhash')

echo "Transaction hash: $TX_HASH"
echo "Waiting for confirmation..."
sleep 6

# Check if successful
CODE=$(zigchaind query tx $TX_HASH --node $NODE --output json | jq -r '.code')
if [ "$CODE" == "0" ]; then
    echo " Deposit successful!"
else
    echo "❌ Deposit failed with code: $CODE"
    exit 1
fi
```

### For Python

```python
import subprocess
import json
import time

def deposit(amount: int):
    cmd = [
        "zigchaind", "tx", "wasm", "execute",
        VAULT_ADDRESS,
        '{"deposit":{}}',
        "--from", WALLET,
        "--amount", f"{amount}{STABLECOIN_DENOM}",
        "--gas", "auto",
        "--gas-adjustment", "1.5",
        "--node", NODE,
        "--chain-id", CHAIN_ID,
        "-y",
        "--output", "json"
    ]
    
    result = subprocess.run(cmd, capture_output=True, text=True)
    tx_data = json.loads(result.stdout)
    tx_hash = tx_data['txhash']
    
    print(f"Transaction hash: {tx_hash}")
    print("Waiting for confirmation...")
    time.sleep(6)
    
    # Check result
    check_cmd = ["zigchaind", "query", "tx", tx_hash, "--node", NODE, "--output", "json"]
    check_result = subprocess.run(check_cmd, capture_output=True, text=True)
    tx_info = json.loads(check_result.stdout)
    
    if tx_info['code'] == 0:
        print(" Deposit successful!")
        return True
    else:
        print(f"❌ Deposit failed with code: {tx_info['code']}")
        return False
```

### For JavaScript/TypeScript

```javascript
import { SigningCosmWasmClient } from "@cosmjs/cosmwasm-stargate";
import { DirectSecp256k1HdWallet } from "@cosmjs/proto-signing";

async function deposit(amount) {
    const wallet = await DirectSecp256k1HdWallet.fromMnemonic(MNEMONIC, {
        prefix: "zig"
    });
    
    const [account] = await wallet.getAccounts();
    
    const client = await SigningCosmWasmClient.connectWithSigner(
        NODE,
        wallet,
        { gasPrice: "0.025uzig" }
    );
    
    const msg = { deposit: {} };
    const funds = [{ denom: STABLECOIN_DENOM, amount: amount.toString() }];
    
    const result = await client.execute(
        account.address,
        VAULT_ADDRESS,
        msg,
        "auto",
        undefined,
        funds
    );
    
    console.log("Transaction hash:", result.transactionHash);
    console.log(" Deposit successful!");
    
    return result;
}
```

---

## Summary

This Token Vault contract provides three main execute messages:

1. **Deposit** - Send stablecoins, receive LP tokens (1:1)
2. **Withdraw** - Send LP tokens, receive stablecoins (1:1)
3. **Update Config** - Admin can change configuration

