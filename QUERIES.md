# Token Vault - Quick Query Reference

Quick reference for querying the Token Vault contract. For detailed documentation, workflow examples, and integration guides, see the [docs/](docs/) folder.

## Quick Start

```bash
# Load configuration
source scripts/vault_addresses.txt

# Query vault state (TVL and LP supply)
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"vault_info":{}}' --node $NODE --output json | jq '.data'

# Query your position
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"user_info\":{\"address\":\"$MY_ADDR\"}}" --node $NODE --output json | jq '.data'

# Query your balances
zigchaind query bank balances $MY_ADDR --node $NODE --output json | jq '.balances'
```

## Available Queries

| Query | Command | What It Returns |
|-------|---------|-----------------|
| **Config** | `'{"config":{}}'` | Stablecoin denom, LP denom, admin |
| **Vault State** | `'{"vault_info":{}}'` | Total deposits, LP supply |
| **User Position** | `'{"user_info":{"address":"..."}}'` | User's LP balance, stablecoin value |
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

## Common Queries

### Get Contract Configuration

```bash
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"config":{}}' --node $NODE --output json | jq '.data'
```

Returns: `stablecoin_denom`, `lp_full_denom`, `admin`

### Get Vault State (TVL)

```bash
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"vault_info":{}}' --node $NODE --output json | jq '.data'
```

Returns: `total_stablecoin_deposited`, `total_lp_supply`

### Get Your Position

```bash
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"user_info\":{\"address\":\"$MY_ADDR\"}}" --node $NODE --output json | jq '.data'
```

Returns: `address`, `lp_balance`, `stablecoin_value`

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
  '{"vault_info":{}}' --node $NODE --output json | jq '.data.total_stablecoin_deposited'
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

### Before Withdrawal

```bash
echo "Your LP balance:"
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"user_info\":{\"address\":\"$MY_ADDR\"}}" --node $NODE --output json | jq '.data.lp_balance'

echo "Vault liquidity:"
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"vault_info":{}}' --node $NODE --output json | jq '.data.total_stablecoin_deposited'
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
  updateTVL(vaultInfo.total_stablecoin_deposited);
}, 10000);

// Query user info after transactions
async function afterDeposit() {
  await new Promise(resolve => setTimeout(resolve, 6000)); // Wait for block
  const userInfo = await client.queryContractSmart(
    contractAddress,
    { user_info: { address: userAddress } }
  );
  updateUserBalance(userInfo.lp_balance);
}
```

### For Analytics/Monitoring
```bash
# Create a monitoring script
#!/bin/bash
while true; do
  echo "$(date): TVL=$(zigchaind query wasm contract-state smart $CONTRACT '{"vault_info":{}}' --node $NODE --output json | jq -r '.data.total_stablecoin_deposited')"
  sleep 60
done
```

### For Bots/Automated Systems
```bash
# Check if vault has sufficient liquidity before withdrawal
MIN_LIQUIDITY=10000
CURRENT=$(zigchaind query wasm contract-state smart $CONTRACT '{"vault_info":{}}' --node $NODE --output json | jq -r '.data.total_stablecoin_deposited')

if [ "$CURRENT" -gt "$MIN_LIQUIDITY" ]; then
  echo "Sufficient liquidity: $CURRENT"
  # Proceed with withdrawal
else
  echo "Insufficient liquidity: $CURRENT"
  exit 1
fi
```

---

## Best Practices

### For Regular Users
1. Always check your balance before withdrawing
2. Verify transactions completed by querying balances after
3. Keep your LP tokens in a secure wallet
4. Don't withdraw during network congestion (higher gas fees)
5. Use the interactive script for safer operations

### For Developers
1. Query vault_info before executing deposits/withdrawals to check liquidity
2. Implement retry logic for network failures
3. Validate user input before broadcasting transactions
4. Cache query results for a few seconds to reduce RPC load
5. Monitor vault_info regularly to track TVL changes
6. Always verify bank balances match contract state

### For Integrators
1. Use the bank module for LP token balance checks (faster)
2. Query user_info only when you need detailed position data
3. Set up monitoring alerts for vault_info changes
4. Implement proper error handling for all query types
5. Consider using WebSocket subscriptions for real-time updates

---
## Summary

This Token Vault provides a secure, straightforward way to manage liquidity with a 1:1 deposit/withdrawal mechanism. The use of native TokenFactory tokens means LP tokens work with any wallet that supports the blockchain, and queries are always free.

The contract handles edge cases gracefully, implements strong security measures, and maintains precise accounting. Whether you're a regular user, developer, or integrator, the query interface provides all the information you need to interact with the vault safely.

Key takeaways:
- Use the interactive script for the easiest experience
- Query before and after transactions to verify success
- LP tokens are tradeable native assets
- All operations are protected by input validation and security checks
- Queries are free and can be called as often as needed

For executing deposits and withdrawals, refer to the main [README.md](README.md) or use `scripts/interact_tokenfactory.sh`.

---

**Version:** 1.0  
**Last Updated:** December 23, 2025  
**Status:** ✅ All Commands Verified & Tested  
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
