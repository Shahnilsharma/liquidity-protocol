#!/bin/bash

# DeFa v2 deployment helper
# Uploads v2 contract WASMs and instantiates PoolFactory with code IDs.

set -euo pipefail

# Configuration
NODE="https://public-zigchain-testnet-rpc.numia.xyz:443"
CHAIN_ID="zig-test-2"
WALLET="test-wallet"
GAS_PRICES="0.0025uzig"
GAS_AUTO="--gas auto --gas-adjustment 1.5"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

CONTRACT_WASMS=(
  "defa_pool_factory"
  "defa_psp_pool"
  "defa_credit_manager"
  "defa_yield_distributor"
  "defa_yield_reserve"
)

echo -e "${BLUE}================================${NC}"
echo -e "${BLUE}DeFa v2 Stack Deployment${NC}"
echo -e "${BLUE}================================${NC}"

ADMIN_ADDR=$(zigchaind keys show "$WALLET" -a)
echo -e "${GREEN}Wallet:${NC} ${WALLET}"
echo -e "${GREEN}Admin:${NC} ${ADMIN_ADDR}"
echo ""

echo -e "${YELLOW}Checking balance...${NC}"
zigchaind query bank balances "$ADMIN_ADDR" --node "$NODE"
echo ""
echo ""

# Check that all required WASM files exist in artifacts/
for wasm in "${CONTRACT_WASMS[@]}"; do
  if [ ! -f "artifacts/${wasm}.wasm" ]; then
    echo -e "${RED}Missing artifacts/${wasm}.wasm. Please build and optimize WASM files before running this script.${NC}"
    exit 1
  fi
done
echo -e "${GREEN}All required WASM artifacts found in artifacts/.${NC}"

# Check that all required WASM files exist in artifacts/
for wasm in "${CONTRACT_WASMS[@]}"; do
  if [ ! -f "artifacts/${wasm}.wasm" ]; then
    echo -e "${RED}Missing artifacts/${wasm}.wasm. Please build and optimize WASM files before running this script.${NC}"
    exit 1
  fi
done
echo -e "${GREEN}All required WASM artifacts found in artifacts/.${NC}"

declare -A CODE_IDS

store_wasm() {
  local wasm_name="$1"
  local wasm_path="artifacts/${wasm_name}.wasm"

  echo -e "${YELLOW}Uploading ${wasm_name}...${NC}"
  local upload_tx
  upload_tx=$(zigchaind tx wasm store "$wasm_path" \
    --from "$WALLET" \
    --node "$NODE" \
    --chain-id "$CHAIN_ID" \
    --gas-prices "$GAS_PRICES" \
    $GAS_AUTO \
    --output json \
    -y)

  local txhash
  txhash=$(echo "$upload_tx" | jq -r '.txhash')

  echo -e "${YELLOW}Waiting for tx ${txhash}...${NC}"
  sleep 6

  local code_id
  code_id=$(zigchaind query tx "$txhash" --node "$NODE" --output json | jq -r '.events[] | select(.type=="store_code") | .attributes[] | select(.key=="code_id") | .value' | head -n1)

  if [ -z "$code_id" ] || [ "$code_id" = "null" ]; then
    echo -e "${RED}Failed to parse code_id for ${wasm_name} from tx ${txhash}${NC}"
    exit 1
  fi

  CODE_IDS["$wasm_name"]="$code_id"
  echo -e "${GREEN}${wasm_name} code_id:${NC} ${code_id}"
  echo ""
}

for wasm in "${CONTRACT_WASMS[@]}"; do
  store_wasm "$wasm"
done

POOL_FACTORY_INIT_MSG=$(cat <<EOF
{
  "admin": "${ADMIN_ADDR}",
  "psp_pool_code_id": ${CODE_IDS[defa_psp_pool]},
  "credit_manager_code_id": ${CODE_IDS[defa_credit_manager]},
  "yield_distributor_code_id": ${CODE_IDS[defa_yield_distributor]},
  "yield_reserve_code_id": ${CODE_IDS[defa_yield_reserve]}
}
EOF
)

echo -e "${YELLOW}Instantiating PoolFactory...${NC}"
instantiate_tx=$(zigchaind tx wasm instantiate "${CODE_IDS[defa_pool_factory]}" "$POOL_FACTORY_INIT_MSG" \
  --from "$WALLET" \
  --label "defa-pool-factory-$(date +%s)" \
  --node "$NODE" \
  --chain-id "$CHAIN_ID" \
  --gas-prices "$GAS_PRICES" \
  $GAS_AUTO \
  --output json \
  --admin "$ADMIN_ADDR" \
  -y)

txhash=$(echo "$instantiate_tx" | jq -r '.txhash')

echo -e "${YELLOW}Waiting for instantiate tx ${txhash}...${NC}"
sleep 6

POOL_FACTORY_ADDRESS=$(zigchaind query tx "$txhash" --node "$NODE" --output json | jq -r '.events[] | select(.type=="instantiate") | .attributes[] | select(.key=="_contract_address") | .value' | head -n1)

if [ -z "$POOL_FACTORY_ADDRESS" ] || [ "$POOL_FACTORY_ADDRESS" = "null" ]; then
  echo -e "${RED}Failed to parse PoolFactory address from tx ${txhash}${NC}"
  exit 1
fi

cat > scripts/v2_addresses.txt <<EOF
# DeFa v2 deployment outputs
# Date: $(date)

export NODE="${NODE}"
export CHAIN_ID="${CHAIN_ID}"
export WALLET="${WALLET}"
export ADMIN_ADDR="${ADMIN_ADDR}"

export POOL_FACTORY_CODE_ID="${CODE_IDS[defa_pool_factory]}"
export PSP_POOL_CODE_ID="${CODE_IDS[defa_psp_pool]}"
export CREDIT_MANAGER_CODE_ID="${CODE_IDS[defa_credit_manager]}"
export YIELD_DISTRIBUTOR_CODE_ID="${CODE_IDS[defa_yield_distributor]}"
export YIELD_RESERVE_CODE_ID="${CODE_IDS[defa_yield_reserve]}"

export POOL_FACTORY_ADDRESS="${POOL_FACTORY_ADDRESS}"
EOF

echo -e "${GREEN}================================${NC}"
echo -e "${GREEN}DeFa v2 deploy complete${NC}"
echo -e "${GREEN}================================${NC}"
echo -e "${BLUE}PoolFactory:${NC} ${POOL_FACTORY_ADDRESS}"
echo -e "${BLUE}PoolFactory code_id:${NC} ${CODE_IDS[defa_pool_factory]}"
echo -e "${BLUE}PSPPool code_id:${NC} ${CODE_IDS[defa_psp_pool]}"
echo -e "${BLUE}CreditManager code_id:${NC} ${CODE_IDS[defa_credit_manager]}"
echo -e "${BLUE}YieldDistributor code_id:${NC} ${CODE_IDS[defa_yield_distributor]}"
echo -e "${BLUE}YieldReserve code_id:${NC} ${CODE_IDS[defa_yield_reserve]}"
echo ""
echo -e "${GREEN}Saved:${NC} scripts/v2_addresses.txt"
echo -e "${YELLOW}Next:${NC} call PoolFactory::CreatePool with init payloads for all four per-pool contracts."
