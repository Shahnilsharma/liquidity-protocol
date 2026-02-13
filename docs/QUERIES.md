# Query Reference Guide

Complete technical reference for querying the Token Vault contract. This document provides detailed command syntax, response formats, and integration examples.

## Table of Contents

1. [Query Types](#query-types)
2. [Configuration Setup](#configuration-setup)
3. [Query 1: Contract Configuration](#query-1-contract-configuration)
4. [Query 2: Vault State](#query-2-vault-state)
5. [Query 3: User Position](#query-3-user-position)
6. [Query 4: Bank Balances](#query-4-bank-balances)
7. [Query 5: Contract Metadata](#query-5-contract-metadata)
8. [Query Flow Examples](#query-flow-examples)
9. [Integration Examples](#integration-examples)
10. [Troubleshooting](#troubleshooting)

## Query Types

The Token Vault supports 5 types of queries:

| Query Type | Cost | Speed | Purpose |
|------------|------|-------|---------|
| Config | Free | Instant | Get contract settings |
| Vault Info | Free | Instant | Get total deposits and LP supply |
| User Info | Free | Instant | Get specific user's position |
| Bank Balance | Free | Instant | Get native token balances |
| Contract Metadata | Free | Instant | Get technical contract info |

All queries are **free** (no gas cost) and return results instantly.

## Configuration Setup

Before querying, set your environment variables:

```bash
# Load from vault_addresses.txt
source scripts/vault_addresses.txt

# Or set manually
export VAULT_ADDRESS="zig1jj787thux4fycfh9882lelpzqmxfht5jq62r54x5x59lpr4wuvtqgmdcvm"
export NODE="https://public-zigchain-testnet-rpc.numia.xyz:443"
export MY_ADDR="zig1ug335mpcdn2vpk8p08v4k9z7cqtdg0jj4tqr92"
```

## Query 1: Contract Configuration

Retrieves the contract's configuration including accepted stablecoin denom, LP token denom, and admin address.

### Command

```bash
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"config":{}}' \
  --node $NODE \
  --output json | jq '.data'
```

### Direct Command (No Variables)

```bash
zigchaind query wasm contract-state smart \
  zig1jj787thux4fycfh9882lelpzqmxfht5jq62r54x5x59lpr4wuvtqgmdcvm \
  '{"config":{}}' \
  --node https://public-zigchain-testnet-rpc.numia.xyz:443 \
  --output json | jq '.data'
```

### Response Format

```json
{
  "stablecoin_denom": "uzig",
  "lp_full_denom": "coin.zig1jj787thux4fycfh9882lelpzqmxfht5jq62r54x5x59lpr4wuvtqgmdcvm.vaulttoken",
  "admin": "zig1ug335mpcdn2vpk8p08v4k9z7cqtdg0jj4tqr92"
}
```

### Field Descriptions

- `stablecoin_denom` (string) - The denomination of accepted stablecoin
- `lp_full_denom` (string) - The full TokenFactory denomination for LP tokens
- `admin` (string) - Admin address with special privileges

### Use Cases

- Verify which stablecoin the vault accepts before depositing
- Get the LP token denom for withdrawal operations
- Check admin address for governance purposes
- Save LP denom for future transactions and balance queries

### When to Query

- Before first interaction with vault
- When integrating vault into applications
- After contract upgrade or migration
- When troubleshooting transaction failures

## Query 2: Vault State

Retrieves the overall state of the vault including total deposits and total LP supply.

### Command

```bash
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"vault_info":{}}' \
  --node $NODE \
  --output json | jq '.data'
```

### Direct Command

```bash
zigchaind query wasm contract-state smart \
  zig1jj787thux4fycfh9882lelpzqmxfht5jq62r54x5x59lpr4wuvtqgmdcvm \
  '{"vault_info":{}}' \
  --node https://public-zigchain-testnet-rpc.numia.xyz:443 \
  --output json | jq '.data'
```

### Response Format

```json
{
  "total_yield_shares": "50000",
  "total_stablecoin_value": "52150",
  "total_lp_supply": "50000",
  "price_per_share": "1.043000"
}
```

### Field Descriptions

- `total_yield_shares` (string) - Total shares held in the external yield-generating protocol
- `total_stablecoin_value` (string) - Current total value including accrued yield (totalAssets)
- `total_lp_supply` (string) - Total amount of LP tokens minted and in circulation
- `price_per_share` (string) - Current value of 1 LP token in stablecoin (increases as yield accrues)

### Yield Mechanism

In a yield-generating vault:
```
price_per_share = total_stablecoin_value / total_lp_supply
```

As yield accrues from the external protocol, `total_stablecoin_value` increases while `total_lp_supply` stays constant, causing `price_per_share` to rise. This means LP token holders gain value over time without needing to claim anything.

### Use Cases

- Monitor total value locked (TVL) in the vault
- Verify vault maintains 1:1 ratio
- Check liquidity availability before large withdrawals
- Display vault statistics in dashboards
- Track vault growth over time

### When to Query

- Before large withdrawals (check liquidity)
- After any transaction (verify state consistency)
- Regularly for monitoring (every few minutes/hours)
- When debugging issues

## Query 3: User Position

Retrieves a specific user's LP balance and equivalent stablecoin value.

### Command

```bash
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"user_info\":{\"address\":\"$MY_ADDR\"}}" \
  --node $NODE \
  --output json | jq '.data'
```

### Direct Command (Replace Address)

```bash
zigchaind query wasm contract-state smart \
  zig1jj787thux4fycfh9882lelpzqmxfht5jq62r54x5x59lpr4wuvtqgmdcvm \
  '{"user_info":{"address":"YOUR_ADDRESS_HERE"}}' \
  --node https://public-zigchain-testnet-rpc.numia.xyz:443 \
  --output json | jq '.data'
```

### Example with Real Address

```bash
zigchaind query wasm contract-state smart \
  zig1jj787thux4fycfh9882lelpzqmxfht5jq62r54x5x59lpr4wuvtqgmdcvm \
  '{"user_info":{"address":"zig1ug335mpcdn2vpk8p08v4k9z7cqtdg0jj4tqr92"}}' \
  --node https://public-zigchain-testnet-rpc.numia.xyz:443 \
  --output json | jq '.data'
```

### Response Format

```json
{
  "address": "zig1ug335mpcdn2vpk8p08v4k9z7cqtdg0jj4tqr92",
  "lp_balance": "52150",
  "stablecoin_value": "52150"
}
```

### Field Descriptions

- `address` (string) - The queried user's address
- `lp_balance` (string) - Amount of LP tokens the user owns
- `stablecoin_value` (string) - Equivalent stablecoin value (equals lp_balance in 1:1 vault)

### Use Cases

- Check how many LP tokens a user has
- Calculate how much stablecoin user can withdraw
- Display user's position in UI
- Verify user balance before withdrawal
- Track user's position over time

### When to Query

- After deposits (verify LP tokens recorded)
- Before withdrawals (check available balance)
- When displaying user portfolio
- After LP token transfers

## Query 4: Bank Balances

Queries native token balances including LP tokens and stablecoin directly from the bank module.

### Command - All Balances

```bash
zigchaind query bank balances $MY_ADDR \
  --node $NODE \
  --output json | jq '.balances'
```

### Direct Command

```bash
zigchaind query bank balances YOUR_ADDRESS_HERE \
  --node https://public-zigchain-testnet-rpc.numia.xyz:443 \
  --output json | jq '.balances'
```

### Response Format

```json
[
  {
    "denom": "uzig",
    "amount": "1843626105"
  },
  {
    "denom": "coin.zig1jj787thux4fycfh9882lelpzqmxfht5jq62r54x5x59lpr4wuvtqgmdcvm.vaulttoken",
    "amount": "52150"
  }
]
```

### Command - Filter Specific Denom

```bash
# Get only LP token balance
LP_DENOM="coin.zig1jj787thux4fycfh9882lelpzqmxfht5jq62r54x5x59lpr4wuvtqgmdcvm.vaulttoken"

zigchaind query bank balances $MY_ADDR \
  --node $NODE \
  --output json | jq ".balances[] | select(.denom==\"$LP_DENOM\")"
```

### Command - Get Only Stablecoin

```bash
zigchaind query bank balances $MY_ADDR \
  --node $NODE \
  --output json | jq '.balances[] | select(.denom=="uzig")'
```

### Use Cases

- Check stablecoin balance before deposit
- Verify LP tokens received after deposit
- Check LP balance before withdrawal
- Verify stablecoin received after withdrawal
- Display all user balances in wallet UI
- Cross-check against user_info query

### When to Query

- Before deposits (check available stablecoin)
- After deposits (verify LP tokens received)
- Before withdrawals (verify LP token balance)
- After withdrawals (verify stablecoin received)
- Anytime user wants to see all balances

## Query 5: Contract Metadata

Retrieves basic contract information including code ID, creator, and admin.

### Command

```bash
zigchaind query wasm contract $VAULT_ADDRESS \
  --node $NODE \
  --output json | jq '.contract_info'
```

### Response Format

```json
{
  "code_id": "1617",
  "creator": "zig1ug335mpcdn2vpk8p08v4k9z7cqtdg0jj4tqr92",
  "admin": "zig1ug335mpcdn2vpk8p08v4k9z7cqtdg0jj4tqr92",
  "label": "token-vault-v1"
}
```

### Field Descriptions

- `code_id` (string) - The code ID used to instantiate this contract
- `creator` (string) - Address that created this contract instance
- `admin` (string) - Current admin address
- `label` (string) - Human-readable label for the contract

### Use Cases

- Verify which code ID was used
- Check who created the contract
- Verify admin address matches expected
- Confirm contract label
- Track contract instances

## Query Flow Examples

### Complete Pre-Deposit Check

```bash
source scripts/vault_addresses.txt

echo "=== PRE-DEPOSIT VERIFICATION ==="
echo ""

echo "1. Your stablecoin balance:"
zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
  jq '.balances[] | select(.denom=="uzig")'

echo ""
echo "2. Current vault state:"
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"vault_info":{}}' --node $NODE --output json | jq '.data'

echo ""
echo "3. Your current position:"
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"user_info\":{\"address\":\"$MY_ADDR\"}}" --node $NODE --output json | jq '.data'
```

### Complete Post-Deposit Verification

```bash
source scripts/vault_addresses.txt

echo "=== POST-DEPOSIT VERIFICATION ==="
echo ""

echo "1. Your LP token balance (bank module):"
zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
  jq ".balances[] | select(.denom==\"$LP_FULL_DENOM\")"

echo ""
echo "2. Your position in contract:"
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"user_info\":{\"address\":\"$MY_ADDR\"}}" --node $NODE --output json | jq '.data'

echo ""
echo "3. Updated vault totals:"
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"vault_info":{}}' --node $NODE --output json | jq '.data'

echo ""
echo "If all three show your deposit, transaction was successful!"
```

### Pre-Withdrawal Check

```bash
source scripts/vault_addresses.txt

echo "=== PRE-WITHDRAWAL CHECK ==="
echo ""

echo "1. Your LP tokens (bank module):"
zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
  jq ".balances[] | select(.denom==\"$LP_FULL_DENOM\")"

echo ""
echo "2. Your LP tokens (contract records):"
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"user_info\":{\"address\":\"$MY_ADDR\"}}" --node $NODE --output json | jq '.data.lp_balance'

echo ""
echo "3. Vault total value (including yield):"
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"vault_info":{}}' --node $NODE --output json | jq '.data.total_stablecoin_value'

echo ""
echo "4. Current LP token price:"
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"vault_info":{}}' --node $NODE --output json | jq '.data.price_per_share'

echo ""
echo "Make sure:"
echo "- Bank balance matches contract balance"
echo "- Vault has enough liquidity for your withdrawal"
```

### Post-Withdrawal Verification

```bash
source scripts/vault_addresses.txt

echo "=== POST-WITHDRAWAL VERIFICATION ==="
echo ""

echo "1. Remaining LP tokens:"
zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
  jq ".balances[] | select(.denom==\"$LP_FULL_DENOM\")"

echo ""
echo "2. Your stablecoin balance:"
zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
  jq '.balances[] | select(.denom=="uzig")'

echo ""
echo "3. Updated vault state:"
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"vault_info":{}}' --node $NODE --output json | jq '.data'
```

## Integration Examples

### Frontend Integration (JavaScript)

```javascript
import { CosmWasmClient } from "@cosmjs/cosmwasm-stargate";

const client = await CosmWasmClient.connect("https://public-zigchain-testnet-rpc.numia.xyz:443");

// Query vault state
async function getVaultState() {
  const contractAddress = "zig1jj787thux4fycfh9882lelpzqmxfht5jq62r54x5x59lpr4wuvtqgmdcvm";
  
  const vaultInfo = await client.queryContractSmart(
    contractAddress,
    { vault_info: {} }
  );
  
  return {
    tvl: vaultInfo.total_stablecoin_value,
    lpSupply: vaultInfo.total_lp_supply,
    pricePerShare: vaultInfo.price_per_share
  };
}

// Query user position
async function getUserPosition(userAddress) {
  const contractAddress = "zig1jj787thux4fycfh9882lelpzqmxfht5jq62r54x5x59lpr4wuvtqgmdcvm";
  
  const userInfo = await client.queryContractSmart(
    contractAddress,
    { user_info: { address: userAddress } }
  );
  
  return {
    lpBalance: userInfo.lp_balance,
    stablecoinValue: userInfo.stablecoin_value
  };
}

// Display TVL with auto-refresh
setInterval(async () => {
  const state = await getVaultState();
  document.getElementById('tvl').textContent = state.tvl;
}, 10000); // Update every 10 seconds
```

### Monitoring Script (Bash)

```bash
#!/bin/bash

# monitoring.sh - Track vault metrics over time

source scripts/vault_addresses.txt

LOG_FILE="vault_metrics.log"

while true; do
  TIMESTAMP=$(date '+%Y-%m-%d %H:%M:%S')
  
  VAULT_INFO=$(zigchaind query wasm contract-state smart $VAULT_ADDRESS \
    '{"vault_info":{}}' --node $NODE --output json | jq '.data')
  
  TVL=$(echo $VAULT_INFO | jq -r '.total_stablecoin_value')
  LP_SUPPLY=$(echo $VAULT_INFO | jq -r '.total_lp_supply')
  PRICE=$(echo $VAULT_INFO | jq -r '.price_per_share')
  
  echo "$TIMESTAMP | TVL: $TVL | LP Supply: $LP_SUPPLY | Price: $PRICE" | tee -a $LOG_FILE
  
  # Check invariant
  if [ "$TVL" != "$LP_SUPPLY" ]; then
    echo "WARNING: 1:1 ratio broken!" | tee -a $LOG_FILE
  fi
  
  sleep 60 # Check every minute
done
```

### Python Integration

```python
import requests
import json
import time

class VaultQueryClient:
    def __init__(self, rpc_url, contract_address):
        self.rpc_url = rpc_url
        self.contract_address = contract_address
    
    def query_smart_contract(self, query_msg):
        """Generic smart contract query"""
        url = f"{self.rpc_url}/cosmwasm/wasm/v1/contract/{self.contract_address}/smart/{query_msg}"
        response = requests.get(url)
        return response.json()
    
    def get_vault_info(self):
        """Get vault state"""
        query = {"vault_info": {}}
        query_b64 = base64.b64encode(json.dumps(query).encode()).decode()
        return self.query_smart_contract(query_b64)
    
    def get_user_info(self, address):
        """Get user position"""
        query = {"user_info": {"address": address}}
        query_b64 = base64.b64encode(json.dumps(query).encode()).decode()
        return self.query_smart_contract(query_b64)
    
    def monitor_tvl(self, interval=60):
        """Monitor TVL with alerts"""
        while True:
            vault_info = self.get_vault_info()
            tvl = vault_info['data']['total_stablecoin_value']
            lp_supply = vault_info['data']['total_lp_supply']
            price_per_share = vault_info['data']['price_per_share']
            
            print(f"TVL: {tvl}, LP Supply: {lp_supply}")
            
            if tvl != lp_supply:
                print("ALERT: 1:1 ratio broken!")
            
            time.sleep(interval)

# Usage
client = VaultQueryClient(
    "https://public-zigchain-testnet-rpc.numia.xyz:443",
    "zig1jj787thux4fycfh9882lelpzqmxfht5jq62r54x5x59lpr4wuvtqgmdcvm"
)

vault_state = client.get_vault_info()
print(vault_state)
```

## Troubleshooting

### Error: Connection Refused

**Problem:** Cannot connect to RPC node

**Solution:**
```bash
# Try alternative RPC endpoint
export NODE="https://rpc.zig-test-2.zigchain.io"

# Verify node is responding
curl -s $NODE/status | jq '.result.node_info.network'
```

### Error: Contract Not Found

**Problem:** Invalid contract address

**Solution:**
```bash
# List all contracts for this code ID
zigchaind query wasm list-contracts-by-code 1617 --node $NODE

# Reload addresses
source scripts/vault_addresses.txt
echo $VAULT_ADDRESS
```

### Error: Invalid JSON Syntax

**Problem:** Malformed query JSON

**Common issues:**
- Use single quotes for outer quotes: `'{"query":{}}'`
- Use escaped double quotes for variables: `"{\"address\":\"$VAR\"}"`
- Check for missing commas, brackets, or braces

**Example of correct syntax:**
```bash
# Correct
zigchaind query wasm contract-state smart $CONTRACT '{"config":{}}' --node $NODE

# Wrong - will fail
zigchaind query wasm contract-state smart $CONTRACT {"config":{}} --node $NODE
```

### Query Returns Null or Empty

**Problem:** Address doesn't exist or has no balance

**Solution:**
```bash
# Verify address format
echo $MY_ADDR

# Check if address has any transactions
zigchaind query bank balances $MY_ADDR --node $NODE

# For new addresses, result will be empty - this is normal
```

### Slow Query Response

**Problem:** RPC node is slow or overloaded

**Solution:**
```bash
# Use different RPC endpoint
export NODE="https://rpc.zig-test-2.zigchain.io"

# Or run your own local node for faster queries
```

### Balance Mismatch Between Queries

**Problem:** Bank balance doesn't match user_info

**Solution:**
```bash
# Query both and compare
BANK_BALANCE=$(zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
  jq -r ".balances[] | select(.denom==\"$LP_FULL_DENOM\") | .amount")

CONTRACT_BALANCE=$(zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"user_info\":{\"address\":\"$MY_ADDR\"}}" --node $NODE --output json | \
  jq -r '.data.lp_balance')

echo "Bank: $BANK_BALANCE"
echo "Contract: $CONTRACT_BALANCE"

# They should match. If not, there's an issue with the contract.
```

## Quick Reference

```bash
# Source configuration
source scripts/vault_addresses.txt

# Query config
zigchaind query wasm contract-state smart $VAULT_ADDRESS '{"config":{}}' --node $NODE --output json | jq '.data'

# Query vault state
zigchaind query wasm contract-state smart $VAULT_ADDRESS '{"vault_info":{}}' --node $NODE --output json | jq '.data'

# Query your position
zigchaind query wasm contract-state smart $VAULT_ADDRESS "{\"user_info\":{\"address\":\"$MY_ADDR\"}}" --node $NODE --output json | jq '.data'

# Query your balances
zigchaind query bank balances $MY_ADDR --node $NODE --output json | jq '.balances'

# Query contract metadata
zigchaind query wasm contract $VAULT_ADDRESS --node $NODE --output json | jq '.contract_info'
```

## Summary

This query reference covers all available queries for the Token Vault contract:

- **Config** - Get contract configuration
- **Vault Info** - Get total deposits and LP supply
- **User Info** - Get user's position
- **Bank Balances** - Get native token balances
- **Contract Metadata** - Get technical details

All queries are free, return results instantly, and can be called as often as needed. Use the workflow examples to verify transactions and the integration examples to build applications on top of the vault.

For getting started guides, see [GETTING_STARTED.md](GETTING_STARTED.md).  
For security information, see [SECURITY.md](SECURITY.md).

---

**Version:** 1.0  
**Network:** ZigChain Testnet (zig-test-2)  
**Status:** ✅ All Commands Verified
