#!/bin/bash

# Quick Command Reference
# Liquidity Protocol - CosmWasm Contract

# Important: Read docs/PROJECT_STATUS.md first
# ZigChain requires whitelisted address for upload

# BUILD & TEST

# Clean build
cargo clean && cargo wasm

# Run tests
cargo test

# Optimize for deployment
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/optimizer:0.16.1

# Check artifact
ls -lh artifacts/
cat artifacts/checksums.txt

# ========================================
# ZIGCHAIN DEPLOYMENT (Requires Whitelist)
# ========================================

# Check if your address is whitelisted
export WALLET_ADDR=$(zigchaind keys show mywallet -a)
zigchaind query wasm params \
  --node https://public-zigchain-testnet-rpc.numia.xyz/ \
  --chain-id zig-test-2 | grep "$WALLET_ADDR"

# If whitelisted, deploy:
./deploy.sh

# ========================================
# ALTERNATIVE: NEUTRON TESTNET (Open Access)
# ========================================

# Install Neutron
# wget https://github.com/neutron-org/neutron/releases/download/vX.X.X/neutrond
# chmod +x neutrond && sudo mv neutrond /usr/local/bin/

# Setup
export NODE="https://rpc-palvus.pion-1.ntrn.tech"
export CHAIN_ID="pion-1"
export WALLET="testnet_wallet"

# Create wallet
neutrond keys add $WALLET

# Get testnet tokens
# Visit: https://faucet.pion-1.ntrn.tech/
# Enter: $(neutrond keys show $WALLET -a)

# Upload stablecoin
TX=$(neutrond tx wasm store ./cw20_base.wasm \
  --from $WALLET \
  --gas auto --gas-adjustment 1.5 \
  --fees 10000untrn \
  --node $NODE \
  --chain-id $CHAIN_ID \
  -y --output json | jq -r '.txhash')

sleep 6

STABLE_CODE_ID=$(neutrond query tx $TX --node $NODE -o json | \
  jq -r '.events[] | select(.type=="store_code") | .attributes[] | select(.key=="code_id") | .value')

echo "Stablecoin Code ID: $STABLE_CODE_ID"

# Instantiate stablecoin
WALLET_ADDR=$(neutrond keys show $WALLET -a)

TX=$(neutrond tx wasm instantiate $STABLE_CODE_ID \
  "{\"name\":\"Mock USDT\",\"symbol\":\"USDT\",\"decimals\":6,\"initial_balances\":[{\"address\":\"$WALLET_ADDR\",\"amount\":\"1000000000000\"}],\"mint\":{\"minter\":\"$WALLET_ADDR\"}}" \
  --from $WALLET \
  --label "usdt_token" \
  --admin $WALLET_ADDR \
  --node $NODE \
  --chain-id $CHAIN_ID \
  --gas 500000 \
  --fees 15000untrn \
  -y --output json | jq -r '.txhash')

sleep 6

STABLE_ADDR=$(neutrond query tx $TX --node $NODE -o json | \
  jq -r '.events[] | select(.type=="instantiate") | .attributes[] | select(.key=="_contract_address") | .value')

echo "Stablecoin Address: $STABLE_ADDR"

# Upload LP token
TX=$(neutrond tx wasm store ./cw20_base.wasm \
  --from $WALLET \
  --gas auto --gas-adjustment 1.5 \
  --fees 10000untrn \
  --node $NODE \
  --chain-id $CHAIN_ID \
  -y --output json | jq -r '.txhash')

sleep 6

LP_CODE_ID=$(neutrond query tx $TX --node $NODE -o json | \
  jq -r '.events[] | select(.type=="store_code") | .attributes[] | select(.key=="code_id") | .value')

echo "LP Token Code ID: $LP_CODE_ID"

# Instantiate LP token
TX=$(neutrond tx wasm instantiate $LP_CODE_ID \
  "{\"name\":\"LP Pool Token\",\"symbol\":\"LPUSDT\",\"decimals\":6,\"initial_balances\":[],\"mint\":{\"minter\":\"$WALLET_ADDR\"}}" \
  --from $WALLET \
  --label "lp_token" \
  --admin $WALLET_ADDR \
  --node $NODE \
  --chain-id $CHAIN_ID \
  --gas 500000 \
  --fees 15000untrn \
  -y --output json | jq -r '.txhash')

sleep 6

LP_ADDR=$(neutrond query tx $TX --node $NODE -o json | \
  jq -r '.events[] | select(.type=="instantiate") | .attributes[] | select(.key=="_contract_address") | .value')

echo "LP Token Address: $LP_ADDR"

# Upload LP pool
TX=$(neutrond tx wasm store ./artifacts/liquidity_protocol.wasm \
  --from $WALLET \
  --gas auto --gas-adjustment 1.5 \
  --fees 15000untrn \
  --node $NODE \
  --chain-id $CHAIN_ID \
  -y --output json | jq -r '.txhash')

sleep 6

POOL_CODE_ID=$(neutrond query tx $TX --node $NODE -o json | \
  jq -r '.events[] | select(.type=="store_code") | .attributes[] | select(.key=="code_id") | .value')

echo "Pool Code ID: $POOL_CODE_ID"

# Instantiate LP pool
TX=$(neutrond tx wasm instantiate $POOL_CODE_ID \
  "{\"stablecoin_address\":\"$STABLE_ADDR\",\"lp_token_address\":\"$LP_ADDR\",\"admin\":\"$WALLET_ADDR\"}" \
  --from $WALLET \
  --label "lp_pool" \
  --admin $WALLET_ADDR \
  --node $NODE \
  --chain-id $CHAIN_ID \
  --gas 500000 \
  --fees 15000untrn \
  -y --output json | jq -r '.txhash')

sleep 6

POOL_ADDR=$(neutrond query tx $TX --node $NODE -o json | \
  jq -r '.events[] | select(.type=="instantiate") | .attributes[] | select(.key=="_contract_address") | .value')

echo "Pool Address: $POOL_ADDR"

# Update LP minter
neutrond tx wasm execute $LP_ADDR \
  "{\"update_minter\":{\"new_minter\":\"$POOL_ADDR\"}}" \
  --from $WALLET \
  --node $NODE \
  --chain-id $CHAIN_ID \
  --gas 300000 \
  --fees 10000untrn \
  -y

sleep 5

# Save addresses
cat > deployed_addresses.txt << EOF
STABLE_ADDR=$STABLE_ADDR
LP_ADDR=$LP_ADDR
POOL_ADDR=$POOL_ADDR
WALLET_ADDR=$WALLET_ADDR
NODE=$NODE
CHAIN_ID=$CHAIN_ID
EOF

echo "========================================="
echo "✅ DEPLOYMENT COMPLETE!"
echo "========================================="
echo ""
echo "Stablecoin: $STABLE_ADDR"
echo "LP Token:   $LP_ADDR"
echo "Pool:       $POOL_ADDR"
echo ""
echo "Test with: source deployed_addresses.txt"

# ========================================
# TESTING
# ========================================

# Source the addresses
# source deployed_addresses.txt

# 1. Check USDT balance
# neutrond query wasm contract-state smart $STABLE_ADDR \
#   "{\"balance\":{\"address\":\"$WALLET_ADDR\"}}" \
#   --node $NODE --chain-id $CHAIN_ID

# 2. Approve pool to spend USDT
# neutrond tx wasm execute $STABLE_ADDR \
#   "{\"increase_allowance\":{\"spender\":\"$POOL_ADDR\",\"amount\":\"1000000000\"}}" \
#   --from $WALLET --node $NODE --chain-id $CHAIN_ID \
#   --gas 300000 --fees 10000untrn -y

# 3. Deposit to pool
# neutrond tx wasm execute $POOL_ADDR \
#   "{\"deposit\":{\"amount\":\"1000000000\"}}" \
#   --from $WALLET --node $NODE --chain-id $CHAIN_ID \
#   --gas 400000 --fees 15000untrn -y

# 4. Check LP balance
# neutrond query wasm contract-state smart $LP_ADDR \
#   "{\"balance\":{\"address\":\"$WALLET_ADDR\"}}" \
#   --node $NODE --chain-id $CHAIN_ID

# 5. Query pool info
# neutrond query wasm contract-state smart $POOL_ADDR \
#   "{\"pool_info\":{}}" \
#   --node $NODE --chain-id $CHAIN_ID

# 6. Approve LP tokens for withdrawal
# neutrond tx wasm execute $LP_ADDR \
#   "{\"increase_allowance\":{\"spender\":\"$POOL_ADDR\",\"amount\":\"500000000\"}}" \
#   --from $WALLET --node $NODE --chain-id $CHAIN_ID \
#   --gas 300000 --fees 10000untrn -y

# 7. Withdraw from pool
# neutrond tx wasm execute $POOL_ADDR \
#   "{\"withdraw\":{\"amount\":\"500000000\"}}" \
#   --from $WALLET --node $NODE --chain-id $CHAIN_ID \
#   --gas 400000 --fees 15000untrn -y

# ========================================
# USEFUL QUERIES
# ========================================

# List all code
# neutrond query wasm list-code --node $NODE --chain-id $CHAIN_ID

# Get contract info
# neutrond query wasm contract $POOL_ADDR --node $NODE --chain-id $CHAIN_ID

# Get contract state
# neutrond query wasm contract-state all $POOL_ADDR --node $NODE --chain-id $CHAIN_ID
