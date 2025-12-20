# Query Guide - Liquidity Pool Contract

Complete reference for querying the TokenFactory-based liquidity pool contract.

## Setup

Load contract addresses:
```bash
source scripts/contract_addresses_tokenfactory.txt
```

## Available Queries

### 1. Config Query

Get contract configuration including admin and token denoms.

```bash
zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
  '{"config":{}}' \
  --node $NODE \
  --output json | jq '.data'
```

**Response:**
```json
{
  "stablecoin_denom": "uzig",
  "lp_full_denom": "coin.zig1zj22cuztwnn60n5edn7mpsgpty4q5wwk4thm0jpv62w73vrssn8snzfsn8.lptoken",
  "admin": "zig1ug335mpcdn2vpk8p08v4k9z7cqtdg0jj4tqr92"
}
```

---

### 2. Pool Info Query

Get total pool statistics including deposits, LP supply, and exchange rate.

```bash
zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
  '{"pool_info":{}}' \
  --node $NODE \
  --output json | jq '.data'
```

**Response:**
```json
{
  "total_stablecoin_deposited": "50",
  "total_lp_supply": "50",
  "exchange_rate": "1.000000"
}
```

**Fields:**
- `total_stablecoin_deposited`: Total stablecoin currently in pool
- `total_lp_supply`: Total LP tokens in circulation
- `exchange_rate`: Current conversion rate (stablecoin per LP token)

---

### 3. User Info Query

Get specific user's LP balance and stablecoin value.

```bash
zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
  "{\"user_info\":{\"address\":\"$MY_ADDR\"}}" \
  --node $NODE \
  --output json | jq '.data'
```

**Or with specific address:**
```bash
zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
  '{"user_info":{"address":"zig1ug335mpcdn2vpk8p08v4k9z7cqtdg0jj4tqr92"}}' \
  --node $NODE \
  --output json | jq '.data'
```

**Response:**
```json
{
  "address": "zig1ug335mpcdn2vpk8p08v4k9z7cqtdg0jj4tqr92",
  "lp_balance": "50",
  "stablecoin_value": "50"
}
```

**Fields:**
- `lp_balance`: User's LP token balance (queried from bank module)
- `stablecoin_value`: Equivalent stablecoin value at current exchange rate

---

### 4. Bank Balance Query

Query LP token balance directly from the bank module (native approach).

```bash
zigchaind query bank balances $MY_ADDR \
  --node $NODE \
  --output json | jq '.balances[] | select(.denom | contains("lptoken"))'
```

**Response:**
```json
{
  "denom": "coin.zig1zj22cuztwnn60n5edn7mpsgpty4q5wwk4thm0jpv62w73vrssn8snzfsn8.lptoken",
  "amount": "50"
}
```

**All balances (including uzig):**
```bash
zigchaind query bank balances $MY_ADDR --node $NODE
```

---

## Query Sequence for Testing

Run these in order to verify complete contract state:

```bash
# Load environment
source scripts/contract_addresses_tokenfactory.txt

# 1. Check configuration
echo "=== CONFIG ===" && \
zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
  '{"config":{}}' --node $NODE --output json | jq '.data'

# 2. Check pool state
echo -e "\n=== POOL INFO ===" && \
zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
  '{"pool_info":{}}' --node $NODE --output json | jq '.data'

# 3. Check your position
echo -e "\n=== YOUR INFO ===" && \
zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
  "{\"user_info\":{\"address\":\"$MY_ADDR\"}}" \
  --node $NODE --output json | jq '.data'

# 4. Check all your balances
echo -e "\n=== YOUR BALANCES ===" && \
zigchaind query bank balances $MY_ADDR --node $NODE
```

---

## Contract Addresses

**Testnet (zig-test-2):**
- Contract: `zig1zj22cuztwnn60n5edn7mpsgpty4q5wwk4thm0jpv62w73vrssn8snzfsn8`
- LP Denom: `coin.zig1zj22cuztwnn60n5edn7mpsgpty4q5wwk4thm0jpv62w73vrssn8snzfsn8.lptoken`
- Code ID: `1611`

---

## Execute Commands

For deposit and withdrawal commands, see:
- [Deployment Guide](DEPLOYMENT_GUIDE.md)
- [Migration Guide](../TOKENFACTORY_MIGRATION.md)

Quick reference:

**Deposit:**
```bash
zigchaind tx wasm execute $LP_POOL_ADDRESS \
  '{"deposit":{}}' \
  --from $WALLET \
  --amount 100uzig \
  --node $NODE --chain-id $CHAIN_ID \
  --gas-prices 0.025uzig --gas auto --gas-adjustment 1.5 -y
```

**Withdraw:**
```bash
zigchaind tx wasm execute $LP_POOL_ADDRESS \
  '{"withdraw":{}}' \
  --from $WALLET \
  --amount 50${LP_FULL_DENOM} \
  --node $NODE --chain-id $CHAIN_ID \
  --gas-prices 0.025uzig --gas auto --gas-adjustment 1.5 -y
```
