# Yield-Generating Vault - Quick Query Reference

Quick reference for querying the Yield-Generating Vault contract. This vault automatically invests deposits into an external lending protocol to earn yield. For detailed documentation, workflow examples, and integration guides, see the [docs/](docs/) folder.

## Quick Start

```bash
# Load configuration
source scripts/vault_addresses.txt

# Query vault state (yield shares, total value with yield, price per share)
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"vault_info":{}}' --node $NODE --output json | jq '.data'

# Query your position (includes accrued yield)
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"user_info\":{\"address\":\"$MY_ADDR\"}}" --node $NODE --output json | jq '.data'

# Query your pending withdrawals
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"pending_withdrawals\":{\"address\":\"$MY_ADDR\"}}" --node $NODE --output json | jq '.data'

# Query your balances
zigchaind query bank balances $MY_ADDR --node $NODE --output json | jq '.balances'
```

## Available Queries

| Query | Command | What It Returns |
|-------|---------|-----------------|
| **Config** | `'{"config":{}}'` | Stablecoin denom, LP denom, admin, withdrawal delay, yield contract |
| **Vault State** | `'{"vault_info":{}}'` | Yield shares, total value with yield, LP supply, price per share |
| **User Position** | `'{"user_info":{"address":"..."}}'` | User's LP balance, value including accrued yield |
| **Pending Withdrawals** | `'{"pending_withdrawals":{"address":"..."}}'` | All pending withdrawals for a user |
| **Specific Withdrawal** | `'{"withdrawal":{"address":"...","withdrawal_id":0}}'` | Details of a specific pending withdrawal |
| **Bank Balance** | `query bank balances <addr>` | All native token balances |

## Setup

Set your environment variables:

```bash
# Load from vault_addresses.txt
source scripts/vault_addresses.txt

# Or set manually:
export VAULT_ADDRESS="zig1jj787thux4fycfh9882lelpzqmxfht5jq62r54x5x59lpr4wuvtqgmdcvm"
export NODE="https://public-zigchain-testnet-rpc.numia.xyz:443"
export MY_ADDR="zig1ug335mpcdn2vpk8p08v4k9z7cqtdg0jj4tqr92"
```

---

## Contract Instantiation Requirements

**CRITICAL**: When deploying this contract, you MUST follow these requirements:

### Required Parameters

| Parameter | Type | Requirement | Description |
|-----------|------|-------------|-------------|
| `stablecoin_denom` | string | Required | Denom of deposit token (e.g., "uzig") |
| `lp_subdenom` | string | **3-44 characters**, start with lowercase | LP token subdenom (e.g., "lptoken", "vault", "lp123") |
| `lp_minting_cap` | u128 | Required, > 0 | Maximum LP token supply |
| `withdrawal_delay_seconds` | u64 | **REQUIRED**, 120-2,592,000 | Time lock duration (2 min - 30 days) |
| `yield_contract_address` | string | **REQUIRED** | Address of external yield-generating lending protocol |

### Common Instantiation Errors

**Error: "Invalid subdenom - Subdenom must be 3-44 characters"**
- ❌ **WRONG**: `"lp_subdenom": "vt"` (only 2 characters)
- ❌ **WRONG**: `"lp_subdenom": "LP"` (must start with lowercase)
- ✅ **CORRECT**: `"lp_subdenom": "lptoken"` (7 characters, lowercase start)
- ✅ **CORRECT**: `"lp_subdenom": "vault"` (5 characters)

**Error: "Invalid withdrawal delay"**
- ❌ **WRONG**: `"withdrawal_delay_seconds": 60` (below 120 minimum)
- ❌ **WRONG**: Missing `withdrawal_delay_seconds` (field is REQUIRED)
- ✅ **CORRECT**: `"withdrawal_delay_seconds": 120` (2 minutes minimum)
- ✅ **CORRECT**: `"withdrawal_delay_seconds": 172800` (2 days)

### Example Instantiation

```bash
zigchaind tx wasm instantiate $CODE_ID '{
  "stablecoin_denom": "uzig",
  "lp_subdenom": "lptoken",
  "lp_minting_cap": "10000000000000",
  "can_change_minting_cap": false,
  "withdrawal_delay_seconds": 120,
  "yield_contract_address": "zig188jwfa9tcxed2wdav5faj6vslp0vsq5lnu9yyn0wwg75fmkxv5ussdw5nu",
  "description": "My Yield Vault LP Token"
}' \
  --from $WALLET \
  --amount 100000000uzig \
  --label "my-vault-v1" \
  --node $NODE \
  --chain-id $CHAIN_ID \
  --gas auto --gas-adjustment 1.5 \
  -y
```

**Note**: The 100000000uzig is the TokenFactory denom creation fee, not a deposit.

---

## Time-Locked Withdrawal Flow

This vault implements a **2-step withdrawal process** with a **configurable time lock** (2 minutes to 30 days) for enhanced security.

**Time Lock Configuration:**
- **REQUIRED** at deployment (no default)
- **Range:** 120 seconds (2 minutes) to 2,592,000 seconds (30 days)
- **IMMUTABLE** after deployment
- Common values: 120s (testing), 3600s (1 hour), 86400s (1 day), 172800s (2 days), 604800s (7 days)

### Step 1: Request Withdrawal

When you request a withdrawal:
1. Your LP tokens are burned immediately
2. A pending withdrawal is created with a unique ID
3. The withdrawal is locked for the configured delay (set at deployment)
4. Your funds remain safe in the contract

**Check your pending withdrawals:**
```bash
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"pending_withdrawals\":{\"address\":\"$MY_ADDR\"}}" \
  --node $NODE --output json | jq '.data'
```

### Step 2: Claim Withdrawal (After Time Lock)

After the configured time lock expires (check contract config for exact delay):
1. Query to verify the withdrawal is claimable
2. Execute the claim transaction with the withdrawal ID
3. Receive your stablecoins

**Check if ready to claim:**
```bash
# Check specific withdrawal
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"withdrawal\":{\"address\":\"$MY_ADDR\",\"withdrawal_id\":0}}" \
  --node $NODE --output json | jq '.data.withdrawal.claimable'
```

**Why Time-Locked Withdrawals?**
- **Security**: Protects against flash loan attacks and exploits
- **Safety**: Gives time to detect and respond to unauthorized access
- **Transparency**: All pending withdrawals are publicly visible
- **Fairness**: Prevents front-running and market manipulation
- **Flexibility**: Delay configured at deployment (2 min to 30 days) based on security needs

**Using the Interactive Script:**

The `scripts/interact_tokenfactory.sh` script makes this easy:
- Option 2: Request Withdrawal (creates pending withdrawal)
- Option 7: Query Pending Withdrawals (shows status and time remaining)
- Option 3: Claim Withdrawal (claims after time lock expires)
- Option 4: Query Config (view configured withdrawal delay)

---

## Execute Messages Reference

While this document focuses on queries, here's a quick reference for execute messages:

### 1. Deposit
```bash
zigchaind tx wasm execute $CONTRACT_ADDRESS \
  '{"deposit":{}}' \
  --from $WALLET \
  --amount "${AMOUNT}uzig" \
  --node $NODE --chain-id $CHAIN_ID \
  --gas-prices 0.025uzig --gas auto --gas-adjustment 1.5 -y
```
**Effect**: 
- Deposits your ZIG into the vault
- Vault automatically invests ZIG into yield protocol
- Mints LP tokens at current share price
- **First deposit: 1:1 ratio**
- **Subsequent deposits: LP tokens based on price per share**
- Example: If price = 1.025, depositing 1,025 ZIG gives you 1,000 LP tokens

### 2. Request Withdrawal
```bash
zigchaind tx wasm execute $CONTRACT_ADDRESS \
  '{"request_withdraw":{}}' \
  --from $WALLET \
  --amount "${AMOUNT}${LP_DENOM}" \
  --node $NODE --chain-id $CHAIN_ID \
  --gas-prices 0.025uzig --gas auto --gas-adjustment 1.5 -y
```
**Effect**: 
- Burns LP tokens immediately
- Calculates your share of vault value **including all accrued yield**
- Creates time-locked pending withdrawal
- Example: 1,000 LP tokens at price 1.025 = 1,025 ZIG withdrawal (includes 25 ZIG yield)

### 3. Claim Withdrawal
```bash
zigchaind tx wasm execute $CONTRACT_ADDRESS \
  "{\"claim_withdraw\":{\"withdrawal_id\":0}}" \
  --from $WALLET \
  --node $NODE --chain-id $CHAIN_ID \
  --gas-prices 0.025uzig --gas auto --gas-adjustment 1.5 -y
```
**Effect**: 
- Claims pending withdrawal after time lock expires
- Vault automatically withdraws from yield protocol if needed
- Sends ZIG tokens to user (principal + all accrued yield)
- Example: Receive 1,025 ZIG from 1,000 LP tokens (2.5% yield earned)

---

## Common Queries

### Get Contract Configuration

```bash
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"config":{}}' --node $NODE --output json | jq '.data'
```

Returns: `stablecoin_denom`, `lp_full_denom`, `admin`, `withdrawal_delay` (in seconds), `yield_contract_address`

**Example output:**
```json
{
  "stablecoin_denom": "uzig",
  "lp_full_denom": "coin.zig1...",
  "admin": "zig1...",
  "withdrawal_delay": 172800,
  "yield_contract_address": "zig188jwfa9tcxed2wdav5faj6vslp0vsq5lnu9yyn0wwg75fmkxv5ussdw5nu"
}
```

Note: `withdrawal_delay` shows the configured time lock in seconds. This value was set at deployment and is immutable.
Common values: 120 (2 min), 3600 (1 hour), 86400 (1 day), 172800 (2 days), 604800 (7 days)

### Get Vault State (TVL with Yield)

```bash
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"vault_info":{}}' --node $NODE --output json | jq '.data'
```

Returns: `total_yield_shares`, `total_stablecoin_value`, `total_lp_supply`, `total_pending_withdrawals`, `price_per_share`

**Example output:**
```json
{
  "total_yield_shares": "1000000",
  "total_stablecoin_value": "1025000",
  "total_lp_supply": "1000000",
  "total_pending_withdrawals": "0",
  "price_per_share": "1.025000"
}
```

**Understanding the Response:**
- `total_yield_shares`: Vault's shares in the external yield protocol
- `total_stablecoin_value`: Current value of all deposits **including accrued yield** (1,025,000 = 1,000,000 principal + 25,000 yield)
- `price_per_share`: Current LP token value (1.025 = 2.5% yield earned)
- **The price per share increases over time as yield accrues!**

### Get Your Position (Including Yield)

```bash
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"user_info\":{\"address\":\"$MY_ADDR\"}}" --node $NODE --output json | jq '.data'
```

Returns: `address`, `lp_balance`, `stablecoin_value` (includes accrued yield)

**Example output:**
```json
{
  "address": "zig1...",
  "lp_balance": "500000",
  "stablecoin_value": "512500"
}
```

**Understanding Your Position:**
- You deposited: 500,000 ZIG (received 500,000 LP tokens at 1:1 initially)
- Current value: 512,500 ZIG (includes 12,500 ZIG yield = 2.5% return)
- Your LP tokens are now worth 1.025 ZIG each (price per share increased)
- **When you withdraw, you receive your principal + all accrued yield**

### Get Your Pending Withdrawals

```bash
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"pending_withdrawals\":{\"address\":\"$MY_ADDR\"}}" \
  --node $NODE --output json | jq '.data'
```

Returns: List of all pending withdrawals with IDs, amounts, release times, and claimable status

**Example output:**
```json
{
  "address": "zig1...",
  "withdrawals": [
    {
      "id": 0,
      "amount": "100000",
      "release_time": "1738800000000000000",
      "claimable": false
    },
    {
      "id": 1,
      "amount": "50000",
      "release_time": "1738600000000000000",
      "claimable": true
    }
  ]
}
```

Note: `release_time` is in nanoseconds. Divide by 1,000,000,000 for Unix timestamp in seconds.

### Get Specific Withdrawal Details

```bash
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"withdrawal\":{\"address\":\"$MY_ADDR\",\"withdrawal_id\":0}}" \
  --node $NODE --output json | jq '.data'
```

Returns: Details of a specific withdrawal request

### Get All Your Balances

```bash
zigchaind query bank balances $MY_ADDR --node $NODE --output json | jq '.balances'
```

Returns: Array of all token balances (UZIG, LP tokens, etc.)

### Get Only LP Tokens

```bash
zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
  jq ".balances[] | select(.denom | contains(\"vaulttoken\"))"
```

Returns: Your LP token balance

---

## Quick Verification Scripts

### Before Deposit

```bash
echo "Your UZIG balance:"
zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
  jq '.balances[] | select(.denom=="uzig")'

echo "Vault liquidity:"
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"vault_info":{}}' --node $NODE --output json | jq '.data.total_stablecoin_value'
```

### After Deposit

```bash
echo "Your LP tokens:"
zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
  jq ".balances[] | select(.denom | contains(\"vaulttoken\"))"

echo "Your position:"
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"user_info\":{\"address\":\"$MY_ADDR\"}}" --node $NODE --output json | jq '.data'
```

### Before Requesting Withdrawal

```bash
echo "Your LP balance:"
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"user_info\":{\"address\":\"$MY_ADDR\"}}" --node $NODE --output json | jq '.data.lp_balance'

echo "Vault liquidity:"
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"vault_info":{}}' --node $NODE --output json | jq '.data.total_stablecoin_value'
```

### After Requesting Withdrawal

```bash
echo "Your pending withdrawals:"
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"pending_withdrawals\":{\"address\":\"$MY_ADDR\"}}" \
  --node $NODE --output json | jq '.data'

echo "Vault state:"
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"vault_info":{}}' --node $NODE --output json | jq '.data'
```

### Before Claiming Withdrawal

```bash
echo "Check if withdrawal is claimable:"
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"withdrawal\":{\"address\":\"$MY_ADDR\",\"withdrawal_id\":0}}" \
  --node $NODE --output json | jq '.data.withdrawal.claimable'
```

---

## Detailed Documentation

For comprehensive documentation, see the [docs/](docs/) folder:

- **[Getting Started Guide](docs/GETTING_STARTED.md)** - Complete walkthrough for new users
  - What is Token Vault
  - Initial setup instructions  
  - Using the interactive script
  - Understanding LP tokens
  - Common workflows
  - Troubleshooting

- **[Query Reference](docs/QUERIES.md)** - Complete technical query documentation
  - All query types with detailed syntax
  - Response formats and field descriptions
  - Query flow examples (pre/post deposit/withdrawal)
  - Integration examples (JavaScript, Python, Bash)
  - Troubleshooting query issues

- **[Security Documentation](docs/SECURITY.md)** - Security features and edge cases
  - Security architecture (7 layers)
  - Edge case handling (10+ scenarios)
  - Attack vector mitigations
  - Best practices for users
  - Audit considerations

---

## Current Deployment

- **Contract:** `zig1jj787thux4fycfh9882lelpzqmxfht5jq62r54x5x59lpr4wuvtqgmdcvm`
- **Code ID:** 1617
- **Network:** ZigChain Testnet (zig-test-2)
- **RPC:** `https://public-zigchain-testnet-rpc.numia.xyz:443`

---

**Quick Links:**
- [Getting Started](docs/GETTING_STARTED.md) - For new users
- [Query Reference](docs/QUERIES.md) - Technical documentation
- [Security](docs/SECURITY.md) - Security features
- [Main README](README.md) - Project overview

All queries are **free** (no gas cost) and return results instantly.
```

### Error: "invalid character in JSON"
**Problem:** JSON syntax error in query

**Solution:**
- Use single quotes for outer quotes: `'{"query":{}}'`
- Use escaped double quotes for nested: `"{\"query\":{}}"`
- Check for missing commas or brackets

### Query Returns Empty or Null
**Problem:** Address doesn't exist or has no balance

**Solution:**
```bash
# Verify address format
echo $MY_ADDR
# Check if address has made any transactions
zigchaind query bank balances $MY_ADDR --node $NODE
```

---

## Integration Tips

### For Frontend/UI Applications
```javascript
// Query vault info every 10 seconds for live updates
setInterval(async () => {
  const vaultInfo = await client.queryContractSmart(
    contractAddress,
    { vault_info: {} }
  );
  updateTVL(vaultInfo.total_stablecoin_value);  // Includes yield!
  updatePricePerShare(vaultInfo.price_per_share);
  updatePendingWithdrawals(vaultInfo.total_pending_withdrawals);
}, 10000);

// Query user info and pending withdrawals after transactions
async function afterRequestWithdrawal(userAddress) {
  await new Promise(resolve => setTimeout(resolve, 6000)); // Wait for block
  
  // Get pending withdrawals
  const pendingWithdrawals = await client.queryContractSmart(
    contractAddress,
    { pending_withdrawals: { address: userAddress } }
  );
  
  // Find the latest withdrawal
  const latest = pendingWithdrawals.withdrawals[pendingWithdrawals.withdrawals.length - 1];
  
  // Calculate time remaining
  const releaseTime = parseInt(latest.release_time) / 1000000000; // Convert nanoseconds to seconds
  const currentTime = Math.floor(Date.now() / 1000);
  const timeRemaining = releaseTime - currentTime;
  
  displayWithdrawalStatus({
    id: latest.id,
    amount: latest.amount,
    claimable: latest.claimable,
    timeRemaining: timeRemaining
  });
}

// Check if any withdrawals are claimable
async function checkClaimableWithdrawals(userAddress) {
  const result = await client.queryContractSmart(
    contractAddress,
    { pending_withdrawals: { address: userAddress } }
  );
  
  const claimable = result.withdrawals.filter(w => w.claimable);
  if (claimable.length > 0) {
    showClaimNotification(claimable);
  }
}
```

### For Analytics/Monitoring
```bash
# Create a monitoring script
#!/bin/bash
while true; do
  echo "$(date): TVL=$(zigchaind query wasm contract-state smart $CONTRACT '{"vault_info":{}}' --node $NODE --output json | jq -r '.data.total_stablecoin_value') Price=$(zigchaind query wasm contract-state smart $CONTRACT '{"vault_info":{}}' --node $NODE --output json | jq -r '.data.price_per_share')"
  sleep 60
done
```

### For Bots/Automated Systems
```bash
# Check pending withdrawals and claim when ready
MY_ADDR="zig1..."
CONTRACT="zig1..."
NODE="https://public-zigchain-testnet-rpc.numia.xyz:443"

# Get pending withdrawals
PENDING=$(zigchaind query wasm contract-state smart $CONTRACT \
  "{\"pending_withdrawals\":{\"address\":\"$MY_ADDR\"}}" \
  --node $NODE --output json)

# Check for claimable withdrawals
CLAIMABLE=$(echo "$PENDING" | jq '.data.withdrawals[] | select(.claimable==true)')

if [ -n "$CLAIMABLE" ]; then
  # Get withdrawal IDs
  IDS=$(echo "$CLAIMABLE" | jq -r '.id')
  
  for ID in $IDS; do
    echo "Claiming withdrawal ID: $ID"
    zigchaind tx wasm execute $CONTRACT \
      "{\"claim_withdraw\":{\"withdrawal_id\":$ID}}" \
      --from wallet --node $NODE --chain-id zig-test-2 \
      --gas-prices 0.025uzig --gas auto --gas-adjustment 1.5 -y
    sleep 6
  done
else
  echo "No withdrawals ready to claim"
fi
```

---

## Best Practices

### For Regular Users
1. Query the contract config to see the withdrawal delay for this vault
2. **Query price_per_share regularly to track yield accumulation**
3. **Your LP token value increases over time - check user_info for current value**
4. Always check your balance before requesting withdrawals
5. Track your pending withdrawals using query option 7 in the script
6. Note the withdrawal ID and release time when requesting withdrawal
7. Wait for the configured time lock to expire (check config or release_time)
8. **Remember: You receive principal + yield when you withdraw**
9. Keep your LP tokens in a secure wallet (they're tradeable!)
10. Don't request withdrawals during network congestion (higher gas fees)
11. Use the interactive script for safer operations with built-in status checks

### For Developers
1. Query vault_info before executing deposits to check liquidity and price per share
2. **Display current price_per_share prominently in your UI**
3. **Calculate and show users their current value including accrued yield**
4. Query pending_withdrawals to show users their locked funds
5. Display time remaining until withdrawals are claimable
6. **Show yield earned: (current_value - initial_deposit) / initial_deposit**
7. Implement retry logic for network failures
8. Validate user input before broadcasting transactions
9. Cache query results for a few seconds to reduce RPC load
10. **Update price_per_share display every 30-60 seconds**
7. Monitor vault_info regularly to track TVL and pending withdrawal changes
8. Always verify bank balances match contract state
9. Handle the 2-step withdrawal flow in your UI clearly
10. Show countdown timers for locked withdrawals
Query pending_withdrawals to show users their locked funds
4. Display withdrawal status (locked/claimable) prominently in UI
5. Set up monitoring alerts for vault_info changes
6. Implement proper error handling for all query types
7. Consider using WebSocket subscriptions for real-time updates
8. Auto-refresh pending withdrawals to update claimable status
9. Provide notifications when withdrawals become claimable
2. Query user_info only when you need detailed position data
3. Set up monitoring alerts for vault_info changes
4. Implement proper error handling for all query types
5. Consider using WebSocket subscriptions for real-time updates

---
## Summary

This **Yield-Generating Vault** automatically invests deposits into an external lending protocol to earn interest for users. It uses share-based pricing where the price per share increases as yield accrues, combined with time-locked withdrawals for enhanced security. The use of native TokenFactory tokens means LP tokens work with any wallet that supports the blockchain, and queries are always free.

### How Yield Generation Works:
1. **You deposit ZIG** → Vault invests it in lending protocol → You get LP tokens
2. **Yield accrues** → Price per share increases (e.g., 1.00 → 1.025 → 1.05)
3. **You withdraw** → Get back your principal + all accrued yield

### Key Differences from Traditional Vaults:
- ❌ **NOT 1:1**: LP token value increases over time
- ✅ **Yield Included**: Your LP tokens grow in value automatically
- ✅ **Share-Based**: Like how traditional finance vaults work
- ✅ **Transparent**: Query price_per_share anytime to see current value

Key features:
- **Configurable Withdrawal Lock**: Withdrawals require a 2-step process with a time lock (120s to 30 days) configured at deployment
- **Query Pending Withdrawals**: Track all your pending withdrawals with IDs, amounts, and release times
- **Claimable Status**: Easy to see which withdrawals are ready to claim
- **Query Config**: View the withdrawal_delay setting for the vault (immutable after deployment)

The contract handles edge cases gracefully, implements strong security measures, and maintains precise accounting. Whether you're a regular user, developer, or integrator, the query interface provides all the information you need to interact with the vault safely.

Key takeaways:
- Use the interactive script for the easiest experience (includes withdrawal status checks)
- Query contract config to see the withdrawal delay (withdrawal_delay field)
- Query pending withdrawals to track your time-locked funds
- Wait for the configured delay to expire before claiming (check release_time)
- Query before and after transactions to verify success
- LP tokens are tradeable native assets
- All operations are protected by input validation and security checks
- Queries are free and can be called as often as needed

For executing deposits and withdrawals, refer to the main [README.md](README.md) or use `scripts/interact_tokenfactory.sh`.

---

**Version:** 2.0  
**Last Updated:** February 4, 2026  
**Status:** ✅ All Commands Verified & Tested with Time-Locked Withdrawals  
**Network:** ZigChain Testnet (zig-test-2)
| **Config** | `'{"config":{}}'` | Stablecoin denom, LP denom, admin |
| **Vault Info** | `'{"vault_info":{}}'` | Total deposits, LP supply |
| **User Info** | `'{"user_info":{"address":"..."}}'` | User LP balance, stablecoin value |
| **Bank Balance** | `query bank balances <addr>` | All native token balances |
| **Contract Info** | `query wasm contract <addr>` | Code ID, creator, admin, label |

---

## Testnet Reference

**Current Deployment:**
- **Contract:** `zig1jj787thux4fycfh9882lelpzqmxfht5jq62r54x5x59lpr4wuvtqgmdcvm`
- **Code ID:** 1617
- **Network:** zig-test-2
- **RPC:** `https://public-zigchain-testnet-rpc.numia.xyz:443`
- **LP Denom:** `coin.zig1jj787thux4fycfh9882lelpzqmxfht5jq62r54x5x59lpr4wuvtqgmdcvm.vaulttoken`

Use this deployment for testing queries before deploying your own contract.

---

**Version:** 1.0  
**Last Updated:** December 23, 2025  
**Status:** ✅ All Queries Verified Working

*For execute messages (deposit, withdraw), see the main [README.md](README.md)*
