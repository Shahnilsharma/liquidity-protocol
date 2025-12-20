#!/bin/bash

# ZigChain Liquidity Protocol - TokenFactory Edition Deployment Script
# This script deploys the TokenFactory-based LP pool contract to ZigChain testnet

set -e

# Configuration
NODE="https://public-zigchain-testnet-rpc.numia.xyz:443"
CHAIN_ID="zig-test-2"
WALLET="mynewwallet"
GAS_PRICES="0.025uzig"
GAS_AUTO="--gas auto --gas-adjustment 1.5"

# Colors for output
GREEN='\033[0.32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${BLUE}================================${NC}"
echo -e "${BLUE}ZigChain LP Pool - TokenFactory${NC}"
echo -e "${BLUE}================================${NC}"
echo ""

# Get wallet address
MY_ADDR=$(zigchaind keys show $WALLET -a)
echo -e "${GREEN}Wallet:${NC} $MY_ADDR"
echo ""

# Check balance
echo -e "${YELLOW}Checking balance...${NC}"
zigchaind query bank balances $MY_ADDR --node $NODE
echo ""

# Build optimized WASM
echo -e "${YELLOW}Building optimized WASM binary...${NC}"
if docker ps >/dev/null 2>&1; then
    docker run --rm -v "$(pwd)":/code \
      --mount type=volume,source="$(basename "$(pwd)")_cache",target=/target \
      --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
      cosmwasm/optimizer:0.16.1
    
    # Move optimized binary
    if [ -f "artifacts/liquidity_protocol.wasm" ]; then
        sudo mv artifacts/liquidity_protocol.wasm artifacts/liquidity_protocol.wasm.old 2>/dev/null || true
    fi
    sudo mv artifacts/liquidity_protocol-aarch64.wasm artifacts/liquidity_protocol.wasm 2>/dev/null || \
    sudo mv artifacts/liquidity_protocol.wasm artifacts/liquidity_protocol.wasm
else
    echo -e "${YELLOW}Docker not available, using regular build...${NC}"
    cargo build --release --target wasm32-unknown-unknown
    mkdir -p artifacts
    cp target/wasm32-unknown-unknown/release/liquidity_protocol.wasm artifacts/
fi

echo -e "${GREEN}WASM binary ready at artifacts/liquidity_protocol.wasm${NC}"
ls -lh artifacts/liquidity_protocol.wasm
echo ""

# Upload contract
echo -e "${YELLOW}Uploading contract...${NC}"
UPLOAD_TX=$(zigchaind tx wasm store artifacts/liquidity_protocol.wasm \
    --from $WALLET \
    --node $NODE \
    --chain-id $CHAIN_ID \
    --gas-prices $GAS_PRICES \
    $GAS_AUTO \
    --output json \
    -y)

echo "$UPLOAD_TX" | jq '.'
UPLOAD_TXHASH=$(echo "$UPLOAD_TX" | jq -r '.txhash')

echo -e "${YELLOW}Waiting for transaction to be included in a block...${NC}"
sleep 6

# Get code ID
CODE_ID=$(zigchaind query tx $UPLOAD_TXHASH --node $NODE --output json | jq -r '.events[] | select(.type=="store_code") | .attributes[] | select(.key=="code_id") | .value')
echo -e "${GREEN}Contract uploaded with Code ID:${NC} $CODE_ID"
echo ""

# Instantiate contract
echo -e "${YELLOW}Instantiating contract...${NC}"
echo -e "${BLUE}This will create a TokenFactory LP token denom${NC}"
echo ""

# Get user input for stablecoin denom (or use default)
read -p "Enter stablecoin denom (default: uzig): " STABLECOIN_DENOM
STABLECOIN_DENOM=${STABLECOIN_DENOM:-uzig}

read -p "Enter LP subdenom (3-44 chars, lowercase start, default: lptoken): " LP_SUBDENOM
LP_SUBDENOM=${LP_SUBDENOM:-lptoken}

read -p "Enter LP minting cap (default: 1000000000000): " LP_CAP
LP_CAP=${LP_CAP:-1000000000000}

INIT_MSG=$(cat <<EOF
{
  "stablecoin_denom": "$STABLECOIN_DENOM",
  "lp_subdenom": "$LP_SUBDENOM",
  "lp_minting_cap": "$LP_CAP",
  "can_change_minting_cap": false,
  "description": "Liquidity Pool LP Token - TokenFactory Edition",
  "admin": "$MY_ADDR"
}
EOF
)

echo -e "${BLUE}Instantiation message:${NC}"
echo "$INIT_MSG" | jq '.'
echo ""

INSTANTIATE_TX=$(zigchaind tx wasm instantiate $CODE_ID "$INIT_MSG" \
    --from $WALLET \
    --label "lp-pool-tokenfactory-$(date +%s)" \
    --node $NODE \
    --chain-id $CHAIN_ID \
    --gas-prices $GAS_PRICES \
    $GAS_AUTO \
    --output json \
    --no-admin \
    -y)

echo "$INSTANTIATE_TX" | jq '.'
INSTANTIATE_TXHASH=$(echo "$INSTANTIATE_TX" | jq -r '.txhash')

echo -e "${YELLOW}Waiting for transaction to be included in a block...${NC}"
sleep 6

# Get contract address
CONTRACT_ADDR=$(zigchaind query tx $INSTANTIATE_TXHASH --node $NODE --output json | jq -r '.events[] | select(.type=="instantiate") | .attributes[] | select(.key=="_contract_address") | .value')
echo -e "${GREEN}Contract instantiated at:${NC} $CONTRACT_ADDR"
echo ""

# Construct LP full denom
LP_FULL_DENOM="coin.${CONTRACT_ADDR}.${LP_SUBDENOM}"
echo -e "${GREEN}LP Token Denom:${NC} $LP_FULL_DENOM"
echo ""

# Save contract addresses
mkdir -p scripts
cat > scripts/contract_addresses.txt <<EOF
# ZigChain LP Pool - TokenFactory Edition
# Deployed on: $(date)
# Chain: $CHAIN_ID

export LP_POOL_ADDRESS="$CONTRACT_ADDR"
export LP_FULL_DENOM="$LP_FULL_DENOM"
export STABLECOIN_DENOM="$STABLECOIN_DENOM"
export CODE_ID="$CODE_ID"
EOF

echo -e "${GREEN}Contract addresses saved to scripts/contract_addresses.txt${NC}"
echo ""

# Query contract config
echo -e "${YELLOW}Querying contract config...${NC}"
zigchaind query wasm contract-state smart $CONTRACT_ADDR '{"config":{}}' --node $NODE --output json | jq '.'
echo ""

# Query pool info
echo -e "${YELLOW}Querying pool info...${NC}"
zigchaind query wasm contract-state smart $CONTRACT_ADDR '{"pool_info":{}}' --node $NODE --output json | jq '.'
echo ""

# Check if LP denom was created in bank module
echo -e "${YELLOW}Checking if LP denom was created...${NC}"
zigchaind query bank denom-metadata $LP_FULL_DENOM --node $NODE 2>&1 || echo -e "${YELLOW}Note: Metadata query may fail if not set, but denom should exist${NC}"
echo ""

echo -e "${GREEN}================================${NC}"
echo -e "${GREEN}Deployment Complete!${NC}"
echo -e "${GREEN}================================${NC}"
echo ""
echo -e "${BLUE}Contract Address:${NC} $CONTRACT_ADDR"
echo -e "${BLUE}LP Token Denom:${NC} $LP_FULL_DENOM"
echo -e "${BLUE}Stablecoin Denom:${NC} $STABLECOIN_DENOM"
echo ""
echo -e "${YELLOW}To interact with the contract, use:${NC}"
echo -e "  source scripts/contract_addresses.txt"
echo -e "  ./scripts/interact.sh"
echo ""
