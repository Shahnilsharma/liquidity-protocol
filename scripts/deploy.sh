#!/bin/bash

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

NODE="https://public-zigchain-testnet-rpc.numia.xyz/"
CHAIN_ID="zig-test-2"
WALLET_NAME="mynewwallet"
GAS_FEES="25000uzig"

echo -e "${GREEN}LP Pool Contract Deployment Script${NC}"
echo -e "${GREEN}ZigChain Testnet${NC}"
echo ""

echo -e "${YELLOW}[1/9] Checking wallet...${NC}"
if ! zigchaind keys show $WALLET_NAME &> /dev/null; then
    echo -e "${RED}Error: Wallet '$WALLET_NAME' not found!${NC}"
    echo "Please create a wallet first:"
    echo "  zigchaind keys add $WALLET_NAME"
    exit 1
fi

WALLET_ADDRESS=$(zigchaind keys show $WALLET_NAME -a)
echo -e "${GREEN}✓ Wallet found: $WALLET_ADDRESS${NC}"

# Check balance
echo -e "\n${YELLOW}[2/9] Checking wallet balance...${NC}"
BALANCE=$(zigchaind query bank balances $WALLET_ADDRESS --node $NODE -o json | jq -r '.balances[0].amount // "0"')
echo "Balance: $BALANCE uzig"

if [ "$BALANCE" -lt "100000" ]; then
    echo -e "${RED}Warning: Low balance. Visit https://faucet.zigchain.com/${NC}"
fi

# Build contract
echo -e "\n${YELLOW}[3/9] Building contract...${NC}"
cargo wasm
echo -e "${GREEN}✓ Contract built${NC}"

# Optimize contract
echo -e "\n${YELLOW}[4/9] Optimizing contract with Docker...${NC}"
if ! command -v docker &> /dev/null; then
    echo -e "${RED}Error: Docker not found. Please install Docker first.${NC}"
    exit 1
fi

docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/optimizer:0.16.1

echo -e "${GREEN}✓ Contract optimized${NC}"

# Upload stablecoin contract
echo -e "\n${YELLOW}[5/9] Uploading stablecoin contract...${NC}"
if [ ! -f "artifacts/cw20_base.wasm" ]; then
    echo "Downloading cw20_base.wasm..."
    wget -q -O artifacts/cw20_base.wasm https://github.com/CosmWasm/cw-plus/releases/download/v1.1.0/cw20_base.wasm
fi

STABLE_UPLOAD_TX=$(zigchaind tx wasm store ./artifacts/cw20_base.wasm \
  --from ${WALLET_NAME} \
  --gas auto --gas-adjustment 1.3 \
  --fees $GAS_FEES \
  --node $NODE \
  --chain-id $CHAIN_ID \
  -y --output json | jq -r '.txhash')

echo "Waiting for transaction to be indexed..."
sleep 6

STABLECOIN_CODE_ID=$(zigchaind query tx $STABLE_UPLOAD_TX --node $NODE -o json | \
  jq -r '.events[] | select(.type=="store_code") | .attributes[] | select(.key=="code_id") | .value')

echo -e "${GREEN}✓ Stablecoin code uploaded: Code ID $STABLECOIN_CODE_ID${NC}"

# Instantiate stablecoin
echo -e "\n${YELLOW}[6/9] Instantiating stablecoin...${NC}"
STABLE_INIT_TX=$(zigchaind tx wasm instantiate $STABLECOIN_CODE_ID \
  "{\"name\":\"Mock USDT\",\"symbol\":\"USDT\",\"decimals\":6,\"initial_balances\":[{\"address\":\"$WALLET_ADDRESS\",\"amount\":\"1000000000000\"}],\"mint\":{\"minter\":\"$WALLET_ADDRESS\"}}" \
  --from $WALLET_NAME \
  --label "mock_usdt_v1" \
  --admin $WALLET_ADDRESS \
  --node $NODE \
  --chain-id $CHAIN_ID \
  --gas 400000 \
  --fees $GAS_FEES \
  -y --output json | jq -r '.txhash')

sleep 6

STABLECOIN_ADDRESS=$(zigchaind query tx $STABLE_INIT_TX --node $NODE -o json | \
  jq -r '.events[] | select(.type=="instantiate") | .attributes[] | select(.key=="_contract_address") | .value')

echo -e "${GREEN}✓ Stablecoin deployed: $STABLECOIN_ADDRESS${NC}"

# Upload LP token contract
echo -e "\n${YELLOW}[7/9] Uploading LP token contract...${NC}"
LP_UPLOAD_TX=$(zigchaind tx wasm store ./artifacts/cw20_base.wasm \
  --from $WALLET_NAME \
  --gas auto --gas-adjustment 1.3 \
  --fees $GAS_FEES \
  --node $NODE \
  --chain-id $CHAIN_ID \
  -y --output json | jq -r '.txhash')

sleep 6

LP_TOKEN_CODE_ID=$(zigchaind query tx $LP_UPLOAD_TX --node $NODE -o json | \
  jq -r '.events[] | select(.type=="store_code") | .attributes[] | select(.key=="code_id") | .value')

echo -e "${GREEN}✓ LP token code uploaded: Code ID $LP_TOKEN_CODE_ID${NC}"

# Instantiate LP token (with temporary minter)
echo -e "\n${YELLOW}[8/9] Instantiating LP token...${NC}"
LP_INIT_TX=$(zigchaind tx wasm instantiate $LP_TOKEN_CODE_ID \
  "{\"name\":\"LP Pool Token\",\"symbol\":\"LPUSDT\",\"decimals\":6,\"initial_balances\":[],\"mint\":{\"minter\":\"$WALLET_ADDRESS\"}}" \
  --from $WALLET_NAME \
  --label "lp_token_v1" \
  --admin $WALLET_ADDRESS \
  --node $NODE \
  --chain-id $CHAIN_ID \
  --gas 400000 \
  --fees $GAS_FEES \
  -y --output json | jq -r '.txhash')

sleep 6

LP_TOKEN_ADDRESS=$(zigchaind query tx $LP_INIT_TX --node $NODE -o json | \
  jq -r '.events[] | select(.type=="instantiate") | .attributes[] | select(.key=="_contract_address") | .value')

echo -e "${GREEN}✓ LP token deployed: $LP_TOKEN_ADDRESS${NC}"

# Upload LP pool contract
echo -e "\n${YELLOW}[9/9] Uploading and instantiating LP pool contract...${NC}"
POOL_UPLOAD_TX=$(zigchaind tx wasm store ./artifacts/liquidity_protocol.wasm \
  --from $WALLET_NAME \
  --gas auto --gas-adjustment 1.3 \
  --fees $GAS_FEES \
  --node $NODE \
  --chain-id $CHAIN_ID \
  -y --output json | jq -r '.txhash')

sleep 6

LP_POOL_CODE_ID=$(zigchaind query tx $POOL_UPLOAD_TX --node $NODE -o json | \
  jq -r '.events[] | select(.type=="store_code") | .attributes[] | select(.key=="code_id") | .value')

echo -e "${GREEN}✓ LP pool code uploaded: Code ID $LP_POOL_CODE_ID${NC}"

# Instantiate LP pool
POOL_INIT_TX=$(zigchaind tx wasm instantiate $LP_POOL_CODE_ID \
  "{\"stablecoin_address\":\"$STABLECOIN_ADDRESS\",\"lp_token_address\":\"$LP_TOKEN_ADDRESS\",\"admin\":\"$WALLET_ADDRESS\"}" \
  --from $WALLET_NAME \
  --label "lp_pool_v1" \
  --admin $WALLET_ADDRESS \
  --node $NODE \
  --chain-id $CHAIN_ID \
  --gas 400000 \
  --fees $GAS_FEES \
  -y --output json | jq -r '.txhash')

sleep 6

LP_POOL_ADDRESS=$(zigchaind query tx $POOL_INIT_TX --node $NODE -o json | \
  jq -r '.events[] | select(.type=="instantiate") | .attributes[] | select(.key=="_contract_address") | .value')

echo -e "${GREEN}✓ LP pool deployed: $LP_POOL_ADDRESS${NC}"

# Update LP token minter
echo -e "\n${YELLOW}Setting LP token minter to pool contract...${NC}"
zigchaind tx wasm execute $LP_TOKEN_ADDRESS \
  "{\"update_minter\":{\"new_minter\":\"$LP_POOL_ADDRESS\"}}" \
  --from $WALLET_NAME \
  --node $NODE \
  --chain-id $CHAIN_ID \
  --gas 300000 \
  --fees 15000uzig \
  -y > /dev/null

sleep 5
echo -e "${GREEN}✓ Minter updated${NC}"

echo -e "\n${YELLOW}Saving contract addresses...${NC}"
cat > scripts/contract_addresses.txt << EOF
# LP Pool Contract Deployment
# Date: $(date)

STABLECOIN_ADDRESS=$STABLECOIN_ADDRESS
LP_TOKEN_ADDRESS=$LP_TOKEN_ADDRESS
LP_POOL_ADDRESS=$LP_POOL_ADDRESS

# Code IDs
STABLECOIN_CODE_ID=$STABLECOIN_CODE_ID
LP_TOKEN_CODE_ID=$LP_TOKEN_CODE_ID
LP_POOL_CODE_ID=$LP_POOL_CODE_ID

# Configuration
WALLET_ADDRESS=$WALLET_ADDRESS
NODE=$NODE
CHAIN_ID=$CHAIN_ID
EOF

echo -e "${GREEN}✓ Addresses saved to scripts/contract_addresses.txt${NC}"

echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Deployment Successful${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo "Contract Addresses:"
echo "  Stablecoin (USDT): $STABLECOIN_ADDRESS"
echo "  LP Token:          $LP_TOKEN_ADDRESS"
echo "  LP Pool:           $LP_POOL_ADDRESS"
echo ""
echo "Next steps:"
echo "  1. Source the addresses: source scripts/contract_addresses.txt"
echo "  2. Run interaction script: ./scripts/interact.sh"
echo "  3. Or manually test with commands in docs/guides/DEPLOYMENT_GUIDE.md"
echo ""
