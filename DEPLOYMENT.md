# Deployment Guide - ZigChain Liquidity Vault

## Overview

This guide covers deploying the security-hardened liquidity vault with configurable time-locked withdrawals.

---

## Prerequisites

1. **ZigChain CLI** installed and configured
2. **Docker** (for WASM optimization)
3. **Wallet** with sufficient ZIG tokens for gas
4. **RPC Node** access (testnet or mainnet)

---

## Configuration Decisions

### ⚠️ CRITICAL: Withdrawal Delay Configuration

**This is the most important deployment parameter!**

The withdrawal delay is **REQUIRED** at instantiation and **CANNOT BE CHANGED**. Choose carefully based on your security requirements:

#### Recommended Settings

| Use Case | Delay | Seconds | Rationale |
|----------|-------|---------|-----------|
| **High Security Vault** | 7 days | 604,800 | Maximum protection, institutional grade |
| **Standard Vault** | 2 days | 172,800 | Balance of security & UX |
| **Fast Liquidity** | 1 day | 86,400 | Good security, better UX |
| **Testing/Development** | 2 minutes | 120 | Minimum allowed, rapid iteration |

#### Constraints

- **Minimum**: 120 seconds (2 minutes)
- **Maximum**: 2,592,000 seconds (30 days)
- **REQUIRED**: Must be explicitly specified (no default value)

**Example scenarios**:

```bash
# High security DeFi protocol
withdrawal_delay_seconds: 604800  # 7 days

# Standard liquidity vault
withdrawal_delay_seconds: 172800  # 2 days

# Fast-moving trading vault
withdrawal_delay_seconds: 86400   # 1 day

# Testing/development
withdrawal_delay_seconds: 120     # 2 minutes (minimum)
```

---

## Step 1: Build Optimized WASM

### Option A: Using Docker (Recommended)

```bash
# From project root
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/optimizer:0.16.1
```

This creates `artifacts/liquidity_protocol.wasm` (~175KB optimized).

### Option B: Standard Build

```bash
cargo build --release --target wasm32-unknown-unknown
```

Artifact at: `target/wasm32-unknown-unknown/release/liquidity_protocol.wasm`

**Verify build**:
```bash
ls -lh artifacts/liquidity_protocol.wasm
# Should see ~150-200KB
```

---

## Step 2: Upload WASM to Chain

### Testnet Deployment

```bash
# Configuration
NODE="https://public-zigchain-testnet-rpc.numia.xyz:443"
CHAIN_ID="zig-test-2"
WALLET="mynewwallet"

# Upload
zigchaind tx wasm store artifacts/liquidity_protocol.wasm \
  --from $WALLET \
  --node $NODE \
  --chain-id $CHAIN_ID \
  --gas-prices 0.025uzig \
  --gas auto \
  --gas-adjustment 1.5 \
  -y \
  --output json | jq
```

**Extract CODE_ID**:
```bash
# From transaction response
CODE_ID=<your_code_id>

# Or query later
zigchaind query wasm list-code --node $NODE | jq
```

---

## Step 3: Instantiate Contract

### Prepare Instantiation Message

Create `instantiate.json`:

```json
{
  "stablecoin_denom": "uzig",
  "lp_subdenom": "vaulttoken",
  "lp_minting_cap": "1000000000000000",
  "can_change_minting_cap": false,
  "uri": "https://your-domain.com/metadata.json",
  "uri_hash": "",
  "description": "ZIG Liquidity Vault Token",
  "admin": null,
  "withdrawal_delay_seconds": 172800
}
```

### Field Explanations

#### Required Fields

- **`stablecoin_denom`**: The token users deposit
  - Format: Native denom (e.g., `"uzig"`) or TokenFactory denom
  - Example: `"uzig"` for ZIG tokens
  
- **`lp_subdenom`**: Name for your LP token (3-44 chars, start with lowercase)
  - Will become: `coin.{contract_address}.{lp_subdenom}`
  - Example: `"vaulttoken"` → `coin.zig1abc...xyz.vaulttoken`
  
- **`lp_minting_cap`**: Maximum LP tokens that can exist
  - Must be > 0
  - Recommend: 2-10x expected TVL
  - Example: `"1000000000000000"` (1 quadrillion micro-units)

#### Optional But Important Fields

- **`withdrawal_delay_seconds`**: ⚠️ **CRITICAL - IMMUTABLE**
  - **Default**: 172,800 (2 days)
  - **Range**: 3,600 to 2,592,000
  - **Cannot be changed after deployment!**
  - Omit to use default

- **`can_change_minting_cap`**: Can you adjust the cap later?
  - **Default**: `false` (more secure)
  - Set `true` only if you need flexibility
  - Recommend: `false` for production

- **`admin`**: Address that can update config
  - **Default**: Transaction sender
  - Can update stablecoin_denom and transfer admin role
  - **Cannot change withdrawal_delay!**

- **`uri`**, **`uri_hash`**, **`description`**: Token metadata
  - Optional but recommended for wallets/explorers
  - URI should point to JSON metadata

### Instantiate Command

```bash
# Set your parameters
CODE_ID=1234
WITHDRAWAL_DELAY=172800  # 2 days

# Instantiate
zigchaind tx wasm instantiate $CODE_ID \
  "$(cat instantiate.json)" \
  --from $WALLET \
  --label "ZIG Liquidity Vault v1" \
  --admin $MY_ADDR \
  --node $NODE \
  --chain-id $CHAIN_ID \
  --gas-prices 0.025uzig \
  --gas auto \
  --gas-adjustment 1.5 \
  -y \
  --output json | jq
```

**Extract Contract Address**:
```bash
# From transaction events
CONTRACT_ADDRESS="zig1..."

# Or query by code ID
zigchaind query wasm list-contract-by-code $CODE_ID --node $NODE --output json | jq
```

---

## Step 4: Verify Deployment

### Check Configuration

```bash
zigchaind query wasm contract-state smart $CONTRACT_ADDRESS \
  '{"config":{}}' \
  --node $NODE \
  --output json | jq '.data'
```

**Verify**:
- ✅ `stablecoin_denom` is correct
- ✅ `lp_full_denom` is formatted properly
- ✅ `admin` is your address
- ✅ **`withdrawal_delay` is your chosen value** ⚠️

### Check Vault State

```bash
zigchaind query wasm contract-state smart $CONTRACT_ADDRESS \
  '{"vault_info":{}}' \
  --node $NODE \
  --output json | jq '.data'
```

Should show zeros for new deployment:
```json
{
  "total_stablecoin_deposited": "0",
  "total_lp_supply": "0",
  "total_pending_withdrawals": "0"
}
```

---

## Step 5: Save Configuration

Create `scripts/vault_addresses.txt`:

```bash
#!/bin/bash
export VAULT_ADDRESS="zig1your_contract_address"
export LP_FULL_DENOM="coin.zig1your_contract_address.vaulttoken"
export STABLECOIN_DENOM="uzig"
export NODE="https://public-zigchain-testnet-rpc.numia.xyz:443"
export CHAIN_ID="zig-test-2"
export MY_ADDR="zig1your_wallet_address"
```

Make executable:
```bash
chmod +x scripts/vault_addresses.txt
```

---

## Step 6: Test Basic Operations

### 1. Test Deposit

```bash
# Source config
source scripts/vault_addresses.txt

# Deposit 1000 uzig
zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"deposit":{}}' \
  --from $WALLET \
  --amount 1000uzig \
  --node $NODE \
  --chain-id $CHAIN_ID \
  --gas-prices 0.025uzig \
  --gas auto --gas-adjustment 1.5 \
  -y

# Wait 6 seconds for block confirmation
sleep 6

# Verify LP tokens received
zigchaind query bank balances $MY_ADDR --node $NODE | grep $LP_FULL_DENOM
```

### 2. Test Request Withdrawal

```bash
# Request withdrawal of 500 LP tokens
zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"request_withdraw":{}}' \
  --from $WALLET \
  --amount "500${LP_FULL_DENOM}" \
  --node $NODE \
  --chain-id $CHAIN_ID \
  --gas-prices 0.025uzig \
  --gas auto --gas-adjustment 1.5 \
  -y
```

### 3. Check Pending Withdrawals

```bash
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"pending_withdrawals\":{\"address\":\"$MY_ADDR\"}}" \
  --node $NODE --output json | jq '.data'
```

Should show withdrawal with `claimable: false` and future `release_time`.

### 4. Test Claim (After Time Lock)

```bash
# Only works after withdrawal_delay seconds have passed!
zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"claim_withdraw":{"withdrawal_id":0}}' \
  --from $WALLET \
  --node $NODE \
  --chain-id $CHAIN_ID \
  --gas-prices 0.025uzig \
  --gas auto --gas-adjustment 1.5 \
  -y
```

---

## Security Checklist

Before announcing your deployment:

### Pre-Deployment
- [ ] Withdrawal delay is appropriate for your use case
- [ ] Reviewed SECURITY_AUDIT.md
- [ ] Tested on testnet with real time delays
- [ ] Verified all address inputs (no typos!)
- [ ] Documented withdrawal_delay for users
- [ ] Prepared user communication about time locks

### Post-Deployment
- [ ] Verified contract configuration
- [ ] Tested deposit flow
- [ ] Tested request withdrawal flow
- [ ] Confirmed time lock is enforced
- [ ] Saved all addresses securely
- [ ] Updated documentation with contract address
- [ ] Communicated withdrawal delay to users

### User Communication
- [ ] Clearly state withdrawal delay in documentation
- [ ] Explain why time locks exist (security)
- [ ] Show how to check pending withdrawals
- [ ] Provide expected wait times
- [ ] Set up monitoring/notifications for claimable withdrawals

---

## Troubleshooting

### Issue: "Invalid withdrawal delay"

**Problem**: Delay outside allowed range
**Solution**: Use value between 120 and 2,592,000 seconds

```bash
# Valid examples:
withdrawal_delay_seconds: 120       # 2 minutes (minimum)
withdrawal_delay_seconds: 3600      # 1 hour
withdrawal_delay_seconds: 86400     # 1 day
withdrawal_delay_seconds: 172800    # 2 days
withdrawal_delay_seconds: 604800    # 7 days

# Invalid examples:
withdrawal_delay_seconds: 60        # Too short (< 120)
withdrawal_delay_seconds: 10000000  # Too long (> 2,592,000)
# (omitted)                         # ERROR: Required field!
```

### Issue: "Subdenom must be 3-44 characters"

**Problem**: LP subdenom too short/long or wrong format
**Solution**: Use 3-44 chars, start with lowercase letter

```bash
# Valid:
"vaulttoken"
"lptoken"
"vault"

# Invalid:
"lp"           # too short
"VaultToken"   # uppercase start
```

### Issue: "Invalid minting cap"

**Problem**: Minting cap is zero or not set
**Solution**: Set to reasonable value > 0

```bash
lp_minting_cap: "1000000000000000"  # 1 quadrillion
```

### Issue: Cannot claim withdrawal

**Problem**: Time lock not expired
**Solution**: Wait for `withdrawal_delay_seconds` to pass

```bash
# Check release time
zigchaind query wasm contract-state smart $CONTRACT \
  "{\"pending_withdrawals\":{\"address\":\"$ADDR\"}}" \
  --node $NODE | jq '.data.withdrawals[0].release_time'

# Compare with current time
date +%s
```

---

## Mainnet Deployment

**⚠️ CRITICAL DIFFERENCES FROM TESTNET**:

1. **Use Mainnet RPC**:
   ```bash
   NODE="https://mainnet-rpc.zigchain.io"
   CHAIN_ID="zig-mainnet-1"
   ```

2. **Higher withdrawal delay recommended**:
   ```bash
   withdrawal_delay_seconds: 604800  # 7 days for mainnet
   ```

3. **Disable minting cap changes**:
   ```bash
   can_change_minting_cap: false
   ```

4. **Test thoroughly on testnet first**!

5. **Verify contract address multiple times**

6. **Have emergency contact plan for users**

---

## Post-Deployment Operations

### Updating Admin (If Needed)

```bash
zigchaind tx wasm execute $CONTRACT_ADDRESS \
  "{\"update_config\":{\"admin\":\"$NEW_ADMIN_ADDR\"}}" \
  --from $WALLET \
  --node $NODE --chain-id $CHAIN_ID \
  --gas-prices 0.025uzig --gas auto --gas-adjustment 1.5 -y
```

### Monitoring Vault Health

```bash
# Watch vault state
watch -n 10 'zigchaind query wasm contract-state smart $CONTRACT_ADDRESS \
  "{\"vault_info\":{}}" --node $NODE --output json | jq ".data"'
```

### User Support Commands

```bash
# Check user's pending withdrawals
zigchaind query wasm contract-state smart $CONTRACT_ADDRESS \
  "{\"pending_withdrawals\":{\"address\":\"$USER_ADDR\"}}" \
  --node $NODE --output json | jq '.data'

# Check user's LP balance
zigchaind query bank balances $USER_ADDR --node $NODE | grep $LP_FULL_DENOM
```

---

## Additional Resources

- [SECURITY_AUDIT.md](SECURITY_AUDIT.md) - Comprehensive security analysis
- [QUERIES.md](QUERIES.md) - Query reference guide
- [README.md](README.md) - Project overview
- [scripts/interact_tokenfactory.sh](scripts/interact_tokenfactory.sh) - Interactive CLI

---

**Questions?** Review the security audit and test thoroughly on testnet first!

**Remember**: `withdrawal_delay_seconds` is IMMUTABLE. Choose wisely! 🔒
