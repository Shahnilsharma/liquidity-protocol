# Liquidity Protocol - Deployment Guide

## Overview
This is a professional CosmWasm smart contract implementing a liquidity pool protocol with:
- CW20 stablecoin deposits (e.g., USDT)
- LP token minting on deposit  
- LP token burning on withdrawal
- Transfer approval pattern for security
- 1:1 exchange rate (can be customized)

## Prerequisites

### Required Tools
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-unknown-unknown

# Install Docker (for optimization)
# Ubuntu/Debian:
sudo apt-get update && sudo apt-get install docker.io
sudo usermod -aG docker $USER

# Install ZigChain daemon
# Follow: https://docs.zigchain.com/
```

### Wallet Setup
```bash
# Create a wallet if you don't have one
zigchaind keys add mywallet

# Or recover existing wallet
zigchaind keys add mywallet --recover

# Get testnet tokens from faucet
# Visit: https://faucet.zigchain.com/
# Your address: zigchaind keys show mywallet -a
```

## ZigChain Testnet Restrictions

**IMPORTANT**: ZigChain testnet has **permissioned code upload**. Only whitelisted addresses can upload WASM code.

### Check Current Permissions
```bash
zigchaind query wasm params \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2
```

### Options for Deployment

#### Option 1: Request Whitelisting (Recommended for Production)
Contact the ZigChain team to get your address whitelisted:
- Discord: https://discord.gg/zigchain
- Telegram: https://t.me/zigchain  
- Provide your wallet address and use case

#### Option 2: Use Existing Code IDs
If CW20 contracts are already deployed, you can instantiate from existing code IDs:

```bash
# List available code
zigchaind query wasm list-code \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 --limit 50

# Check if code ID is CW20
zigchaind query wasm code-info CODE_ID \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2
```

#### Option 3: Deploy to Alternative Testnet
Consider deploying to:
- **Neutron Testnet** (pion-1) - No upload restrictions
- **Juno Testnet** (uni-6) - Open upload
- **Osmosis Testnet** (osmo-test-5) - Open upload

## Build & Optimize Contract

### 1. Clean Build
```bash
cd /home/muneeb/Desktop/NewFolder/lp-transfer/liquidity-protocol
cargo clean
cargo wasm
```

### 2. Run Tests
```bash
cargo test
```

### 3. Optimize for Deployment
```bash
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/optimizer:0.16.1
```

This creates `artifacts/liquidity_protocol.wasm` (~242KB optimized).

### 4. Verify Checksum
```bash
cat artifacts/checksums.txt
# Should show: 14d73355d1687c415802e5255a3397942e114d70804fd22876b021af373cb4f7
```

## Manual Deployment (After Getting Permissions)

### Configuration
```bash
export NODE="https://public-zigchain-testnet-rpc.numia.xyz/"
export CHAIN_ID="zig-test-2"
export WALLET="mywallet"
export WALLET_ADDR=$(zigchaind keys show $WALLET -a)
```

### Step 1: Upload Stablecoin (CW20 Base)
```bash
# Upload code
TX=$(zigchaind tx wasm store ./cw20_base.wasm \
  --from $WALLET \
  --gas auto --gas-adjustment 1.5 \
  --fees 30000uzig \
  --node $NODE \
  --chain-id $CHAIN_ID \
  -y --output json | jq -r '.txhash')

# Wait and get code ID
sleep 6
STABLE_CODE_ID=$(zigchaind query tx $TX --node $NODE -o json | \
  jq -r '.events[] | select(.type=="store_code") | .attributes[] | select(.key=="code_id") | .value')

echo "Stablecoin Code ID: $STABLE_CODE_ID"
```

### Step 2: Instantiate Stablecoin
```bash
# Instantiate with 1,000,000 USDT initial supply
TX=$(zigchaind tx wasm instantiate $STABLE_CODE_ID \
  "{\"name\":\"Mock USDT\",\"symbol\":\"USDT\",\"decimals\":6,\"initial_balances\":[{\"address\":\"$WALLET_ADDR\",\"amount\":\"1000000000000\"}],\"mint\":{\"minter\":\"$WALLET_ADDR\"}}" \
  --from $WALLET \
  --label "usdt_token" \
  --admin $WALLET_ADDR \
  --node $NODE \
  --chain-id $CHAIN_ID \
  --gas 500000 \
  --fees 30000uzig \
  -y --output json | jq -r '.txhash')

sleep 6
STABLE_ADDR=$(zigchaind query tx $TX --node $NODE -o json | \
  jq -r '.events[] | select(.type=="instantiate") | .attributes[] | select(.key=="_contract_address") | .value')

echo "Stablecoin Address: $STABLE_ADDR"
```

### Step 3: Upload LP Token
```bash
# Same CW20 base for LP token
TX=$(zigchaind tx wasm store ./cw20_base.wasm \
  --from $WALLET \
  --gas auto --gas-adjustment 1.5 \
  --fees 30000uzig \
  --node $NODE \
  --chain-id $CHAIN_ID \
  -y --output json | jq -r '.txhash')

sleep 6
LP_CODE_ID=$(zigchaind query tx $TX --node $NODE -o json | \
  jq -r '.events[] | select(.type=="store_code") | .attributes[] | select(.key=="code_id") | .value')

echo "LP Token Code ID: $LP_CODE_ID"
```

### Step 4: Instantiate LP Token
```bash
TX=$(zigchaind tx wasm instantiate $LP_CODE_ID \
  "{\"name\":\"LP Pool Token\",\"symbol\":\"LPUSDT\",\"decimals\":6,\"initial_balances\":[],\"mint\":{\"minter\":\"$WALLET_ADDR\"}}" \
  --from $WALLET \
  --label "lp_token" \
  --admin $WALLET_ADDR \
  --node $NODE \
  --chain-id $CHAIN_ID \
  --gas 500000 \
  --fees 30000uzig \
  -y --output json | jq -r '.txhash')

sleep 6
LP_ADDR=$(zigchaind query tx $TX --node $NODE -o json | \
  jq -r '.events[] | select(.type=="instantiate") | .attributes[] | select(.key=="_contract_address") | .value')

echo "LP Token Address: $LP_ADDR"
```

### Step 5: Upload LP Pool Contract
```bash
TX=$(zigchaind tx wasm store ./artifacts/liquidity_protocol.wasm \
  --from $WALLET \
  --gas auto --gas-adjustment 1.5 \
  --fees 30000uzig \
  --node $NODE \
  --chain-id $CHAIN_ID \
  -y --output json | jq -r '.txhash')

sleep 6
POOL_CODE_ID=$(zigchaind query tx $TX --node $NODE -o json | \
  jq -r '.events[] | select(.type=="store_code") | .attributes[] | select(.key=="code_id") | .value')

echo "Pool Code ID: $POOL_CODE_ID"
```

### Step 6: Instantiate LP Pool
```bash
TX=$(zigchaind tx wasm instantiate $POOL_CODE_ID \
  "{\"stablecoin_address\":\"$STABLE_ADDR\",\"lp_token_address\":\"$LP_ADDR\",\"admin\":\"$WALLET_ADDR\"}" \
  --from $WALLET \
  --label "lp_pool" \
  --admin $WALLET_ADDR \
  --node $NODE \
  --chain-id $CHAIN_ID \
  --gas 500000 \
  --fees 30000uzig \
  -y --output json | jq -r '.txhash')

sleep 6
POOL_ADDR=$(zigchaind query tx $TX --node $NODE -o json | \
  jq -r '.events[] | select(.type=="instantiate") | .attributes[] | select(.key=="_contract_address") | .value')

echo "Pool Address: $POOL_ADDR"
```

### Step 7: Update LP Token Minter
```bash
# Set pool contract as LP token minter
zigchaind tx wasm execute $LP_ADDR \
  "{\"update_minter\":{\"new_minter\":\"$POOL_ADDR\"}}" \
  --from $WALLET \
  --node $NODE \
  --chain-id $CHAIN_ID \
  --gas 300000 \
  --fees 20000uzig \
  -y
```

### Save Configuration
```bash
cat > deployed_addresses.env << EOF
export STABLE_ADDR="$STABLE_ADDR"
export LP_ADDR="$LP_ADDR"
export POOL_ADDR="$POOL_ADDR"
export WALLET_ADDR="$WALLET_ADDR"
export NODE="$NODE"
export CHAIN_ID="$CHAIN_ID"
EOF

echo "✅ Deployment complete! Source the addresses:"
echo "source deployed_addresses.env"
```

## Testing the Contract

### 1. Check Initial Balance
```bash
source deployed_addresses.env

# Check USDT balance
zigchaind query wasm contract-state smart $STABLE_ADDR \
  "{\"balance\":{\"address\":\"$WALLET_ADDR\"}}" \
  --node $NODE --chain-id $CHAIN_ID
```

### 2. Approve Pool to Spend USDT
```bash
# Approve 10,000 USDT (10000000000 with 6 decimals)
zigchaind tx wasm execute $STABLE_ADDR \
  "{\"increase_allowance\":{\"spender\":\"$POOL_ADDR\",\"amount\":\"10000000000\"}}" \
  --from $WALLET \
  --node $NODE \
  --chain-id $CHAIN_ID \
  --gas 300000 \
  --fees 20000uzig \
  -y
```

### 3. Deposit USDT to Pool
```bash
# Deposit 1,000 USDT (1000000000 with 6 decimals)
zigchaind tx wasm execute $POOL_ADDR \
  "{\"deposit\":{\"amount\":\"1000000000\"}}" \
  --from $WALLET \
  --node $NODE \
  --chain-id $CHAIN_ID \
  --gas 400000 \
  --fees 25000uzig \
  -y

# Wait for confirmation
sleep 6
```

### 4. Check LP Balance
```bash
# Check LP token balance
zigchaind query wasm contract-state smart $LP_ADDR \
  "{\"balance\":{\"address\":\"$WALLET_ADDR\"}}" \
  --node $NODE --chain-id $CHAIN_ID

# Should show 1000000000 LP tokens
```

### 5. Query Pool Info
```bash
# Get pool statistics
zigchaind query wasm contract-state smart $POOL_ADDR \
  "{\"pool_info\":{}}" \
  --node $NODE --chain-id $CHAIN_ID
```

### 6. Approve LP Tokens for Withdrawal
```bash
# Approve pool to burn 500 LP tokens
zigchaind tx wasm execute $LP_ADDR \
  "{\"increase_allowance\":{\"spender\":\"$POOL_ADDR\",\"amount\":\"500000000\"}}" \
  --from $WALLET \
  --node $NODE \
  --chain-id $CHAIN_ID \
  --gas 300000 \
  --fees 20000uzig \
  -y
```

### 7. Withdraw from Pool
```bash
# Withdraw 500 LP tokens to get 500 USDT back
zigchaind tx wasm execute $POOL_ADDR \
  "{\"withdraw\":{\"amount\":\"500000000\"}}" \
  --from $WALLET \
  --node $NODE \
  --chain-id $CHAIN_ID \
  --gas 400000 \
  --fees 25000uzig \
  -y

sleep 6
```

### 8. Verify Final Balances
```bash
# Check USDT balance (should have 500 USDT back)
zigchaind query wasm contract-state smart $STABLE_ADDR \
  "{\"balance\":{\"address\":\"$WALLET_ADDR\"}}" \
  --node $NODE --chain-id $CHAIN_ID

# Check LP balance (should have 500 LP tokens remaining)
zigchaind query wasm contract-state smart $LP_ADDR \
  "{\"balance\":{\"address\":\"$WALLET_ADDR\"}}" \
  --node $NODE --chain-id $CHAIN_ID
```

## Architecture & Code Quality

### Modular Structure ✅
```
src/
├── lib.rs         # Module exports and imports
├── contract.rs    # Core business logic
├── msg.rs         # Message definitions
├── state.rs       # State management
└── error.rs       # Error types
```

### Best Practices Implemented ✅
- ✅ Clear separation of concerns
- ✅ Comprehensive error handling
- ✅ Overflow/underflow protection
- ✅ Proper CW20 approval pattern
- ✅ Well-documented functions
- ✅ Unit tests included
- ✅ Migration support
- ✅ Query endpoints for transparency

### Security Features ✅
- ✅ Amount validation (no zero deposits/withdrawals)
- ✅ Balance checks before operations
- ✅ Admin-only configuration updates
- ✅ Transfer approval pattern for tokens
- ✅ Safe math operations (checked_add/checked_sub)

## Troubleshooting

### "Unauthorized" Error on Upload
- ZigChain testnet requires whitelisting
- Contact team or use alternative testnet

### Gas Estimation Issues
```bash
# Use fixed gas instead of auto
--gas 500000
```

### Transaction Not Found
```bash
# Increase sleep time between operations
sleep 10
```

### Balance Not Updating
```bash
# Query specific height
--height BLOCK_HEIGHT
```

## Contract Interface Summary

### Execute Messages
```rust
Deposit { amount: Uint128 }           // Deposit stablecoin, receive LP
Withdraw { amount: Uint128 }          // Burn LP, receive stablecoin  
UpdateConfig { ... }                  // Admin only
```

### Query Messages
```rust
Config {}                             // Get contract configuration
PoolInfo {}                           // Get pool statistics
UserInfo { address: String }          // Get user's LP balance
```

## Support & Resources

- **ZigChain Docs**: https://docs.zigchain.com/
- **CosmWasm Docs**: https://docs.cosmwasm.com/
- **CW20 Spec**: https://github.com/CosmWasm/cw-plus/tree/main/packages/cw20

## License
Apache 2.0
