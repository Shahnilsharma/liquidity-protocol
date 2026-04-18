#!/bin/bash

# DeFa v2 deployment — production version
#
# This script:
#   1. Builds and uploads all five contract WASMs
#   2. Instantiates PoolFactory (the only contract instantiated directly)
#   3. Calls PoolFactory::CreatePool to spin up each per-pool suite atomically
#      (PSPPool, CreditManager, YieldDistributor, YieldReserve are NEVER
#       instantiated directly — they are created by PoolFactory via reply chain)
#   4. Queries the PoolFactory registry to confirm pool registration
#   5. Writes all contract addresses to scripts/vault_addresses.txt
#
# Usage: bash scripts/deploy_defa_v2.sh [--label <prefix>] [--wallet <name>]

set -euo pipefail

# ─── Configuration ────────────────────────────────────────────────────────────
NODE="${NODE:-https://public-zigchain-testnet-rpc.numia.xyz:443}"
CHAIN_ID="${CHAIN_ID:-zig-test-2}"
WALLET="${WALLET:-test-wallet}"
GAS_PRICES="0.0025uzig"
GAS_ADJUSTMENT="1.5"
GAS_FLAGS="--gas auto --gas-adjustment ${GAS_ADJUSTMENT} --gas-prices ${GAS_PRICES}"

# Default facility label prefix (can be overridden via --label)
LABEL_PREFIX="defa-facility"

# ─── Argument parsing ─────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --label)   LABEL_PREFIX="$2"; shift 2 ;;
    --wallet)  WALLET="$2";       shift 2 ;;
    *) echo "Unknown arg: $1"; exit 1 ;;
  esac
done

# ─── Colors ───────────────────────────────────────────────────────────────────
GREEN='\033[0;32m'; BLUE='\033[0;34m'
YELLOW='\033[1;33m'; RED='\033[0;31m'; NC='\033[0m'

info()    { echo -e "${BLUE}[INFO]${NC}  $*"; }
success() { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC}  $*"; }
fail()    { echo -e "${RED}[FAIL]${NC}  $*"; exit 1; }

# ─── Prerequisites ────────────────────────────────────────────────────────────
command -v zigchaind >/dev/null 2>&1 || fail "zigchaind not found in PATH"
command -v jq        >/dev/null 2>&1 || fail "jq not found in PATH"
command -v cargo     >/dev/null 2>&1 || fail "cargo not found in PATH"

ADMIN_ADDR=$(zigchaind keys show "$WALLET" -a 2>/dev/null) \
  || fail "Wallet '${WALLET}' not found. Run: zigchaind keys add ${WALLET}"

# Map Cargo package names -> WASM artifact filenames (underscores)
declare -A WASM_FILES=(
  [defa_pool_factory]="defa_pool_factory"
  [defa_psp_pool]="defa_psp_pool"
  [defa_credit_manager]="defa_credit_manager"
  [defa_yield_distributor]="defa_yield_distributor"
  [defa_yield_reserve]="defa_yield_reserve"
)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
ARTIFACTS_DIR="${REPO_ROOT}/artifacts"

echo ""
echo -e "${BLUE}══════════════════════════════════════${NC}"
echo -e "${BLUE}  DeFa v2 Stack Deployment${NC}"
echo -e "${BLUE}══════════════════════════════════════${NC}"
info "Node:         ${NODE}"
info "Chain ID:     ${CHAIN_ID}"
info "Wallet:       ${WALLET}"
info "Admin:        ${ADMIN_ADDR}"
info "Gas prices:   ${GAS_PRICES}"
info "Gas adj:      ${GAS_ADJUSTMENT}"
info "Label prefix: ${LABEL_PREFIX}"
echo ""


# ─── 1. Check WASM artifacts ────────────────────────────────────────────────
info "Checking for required WASM artifacts in artifacts/ ..."
for key in "${!WASM_FILES[@]}"; do
  wasm_path="${ARTIFACTS_DIR}/${WASM_FILES[$key]}.wasm"
  if [[ ! -f "$wasm_path" ]]; then
    fail "WASM not found: ${wasm_path}"
  fi
  success "Found: artifacts/${WASM_FILES[$key]}.wasm ($(du -sh "$wasm_path" | cut -f1))"
done
echo ""

# ─── 2. Upload all WASMs ─────────────────────────────────────────────────────
declare -A CODE_IDS

store_wasm() {
  local key="$1"
  local wasm_path="${ARTIFACTS_DIR}/${WASM_FILES[$key]}.wasm"

  info "Uploading ${key}..."
  local result txhash code_id

  result=$(zigchaind tx wasm store "${wasm_path}" \
    --from "${WALLET}" \
    --node "${NODE}" \
    --chain-id "${CHAIN_ID}" \
    ${GAS_FLAGS} \
    --output json \
    -y) || fail "Upload failed for ${key}"

  txhash=$(echo "$result" | jq -r '.txhash')
  [[ -z "$txhash" || "$txhash" == "null" ]] && fail "No txhash for ${key}"

  info "Waiting for tx ${txhash}..."
  sleep 8

  code_id=$(zigchaind query tx "${txhash}" \
    --node "${NODE}" \
    --output json \
    | jq -r '.events[] | select(.type=="store_code") | .attributes[] | select(.key=="code_id") | .value' \
    | head -n1)

  [[ -z "$code_id" || "$code_id" == "null" ]] \
    && fail "Failed to parse code_id for ${key} from tx ${txhash}"

  CODE_IDS["$key"]="$code_id"
  success "${key} code_id: ${code_id}"
  echo ""
}

# Upload PoolFactory first, then the four per-pool contracts
store_wasm defa_pool_factory
store_wasm defa_psp_pool
store_wasm defa_credit_manager
store_wasm defa_yield_distributor
store_wasm defa_yield_reserve

# ─── 3. Instantiate PoolFactory ──────────────────────────────────────────────
info "Instantiating PoolFactory (defa-pool-factory)..."

POOL_FACTORY_INIT=$(cat <<EOF
{
  "admin": "${ADMIN_ADDR}",
  "psp_pool_code_id": ${CODE_IDS[defa_psp_pool]},
  "credit_manager_code_id": ${CODE_IDS[defa_credit_manager]},
  "yield_distributor_code_id": ${CODE_IDS[defa_yield_distributor]},
  "yield_reserve_code_id": ${CODE_IDS[defa_yield_reserve]}
}
EOF
)

pf_tx=$(zigchaind tx wasm instantiate "${CODE_IDS[defa_pool_factory]}" \
  "${POOL_FACTORY_INIT}" \
  --from "${WALLET}" \
  --label "defa-pool-factory-$(date +%s)" \
  --admin "${ADMIN_ADDR}" \
  --node "${NODE}" \
  --chain-id "${CHAIN_ID}" \
  ${GAS_FLAGS} \
  --output json \
  -y) || fail "PoolFactory instantiate failed"

pf_txhash=$(echo "$pf_tx" | jq -r '.txhash')
info "Waiting for PoolFactory instantiate tx ${pf_txhash}..."
sleep 8

POOL_FACTORY_ADDRESS=$(zigchaind query tx "${pf_txhash}" \
  --node "${NODE}" \
  --output json \
  | jq -r '.events[] | select(.type=="instantiate") | .attributes[] | select(.key=="_contract_address") | .value' \
  | head -n1)

[[ -z "$POOL_FACTORY_ADDRESS" || "$POOL_FACTORY_ADDRESS" == "null" ]] \
  && fail "Failed to parse PoolFactory address from tx ${pf_txhash}"

success "PoolFactory deployed: ${POOL_FACTORY_ADDRESS}"
echo ""

# ─── 4. Create pool via PoolFactory::CreatePool ───────────────────────────────
#
# IMPORTANT: pool contracts (PSPPool, CreditManager, YieldDistributor,
# YieldReserve) are NEVER instantiated directly. PoolFactory orchestrates
# their creation atomically via its reply chain.
#
# Callers must supply the four per-pool init payloads.
# This script uses placeholder JSON for demo; override with real payloads.
#
# NOTE: edit PSP_POOL_INIT / CREDIT_MANAGER_INIT / YIELD_DISTRIBUTOR_INIT /
#       YIELD_RESERVE_INIT below with your actual field values before use.

info "Calling PoolFactory::CreatePool (label-prefix: ${LABEL_PREFIX})..."

PSP_POOL_INIT=$(cat <<'EOF'
{
  "stablecoin_denom": "uzig",
  "lp_subdenom": "lptoken",
  "lp_minting_cap": "1000000000000",
  "withdrawal_delay_seconds": 604800
}
EOF
)

CREDIT_MANAGER_INIT=$(cat <<'EOF'
{
  "admin": "__ADMIN__",
  "psp_address": "__PSP__",
  "psp_pool": "__PSP_POOL__",
  "yield_reserve": "__YIELD_RESERVE__",
  "stablecoin_denom": "uzig",
  "drawdown_limit": "500000000000",
  "drawdown_tenor_days": 30,
  "psp_rate_bps_per_day": 8,
  "penalty_rate_bps_per_day": 20
}
EOF
)

# Substitute ADMIN into credit_manager init
CREDIT_MANAGER_INIT=$(echo "$CREDIT_MANAGER_INIT" | sed "s/__ADMIN__/${ADMIN_ADDR}/g")

YIELD_DISTRIBUTOR_INIT=$(cat <<'EOF'
{
  "admin": "__ADMIN__",
  "psp_pool": "__PSP_POOL__",
  "yield_reserve": "__YIELD_RESERVE__",
  "investor_apy_bps": 3000,
  "cycle_days": 30
}
EOF
)
YIELD_DISTRIBUTOR_INIT=$(echo "$YIELD_DISTRIBUTOR_INIT" | sed "s/__ADMIN__/${ADMIN_ADDR}/g")

YIELD_RESERVE_INIT=$(cat <<'EOF'
{
  "admin": "__ADMIN__",
  "credit_manager": "__CREDIT_MANAGER__",
  "yield_distributor": "__YIELD_DISTRIBUTOR__",
  "stablecoin_denom": "uzig"
}
EOF
)
YIELD_RESERVE_INIT=$(echo "$YIELD_RESERVE_INIT" | sed "s/__ADMIN__/${ADMIN_ADDR}/g")

CREATE_POOL_MSG=$(cat <<EOF
{
  "create_pool": {
    "label_prefix": "${LABEL_PREFIX}",
    "psp_pool_init_msg": $(echo -n "$PSP_POOL_INIT" | base64 -w 0 | python3 -c "import sys,json; print(json.dumps(sys.stdin.read()))"),
    "credit_manager_init_msg": $(echo -n "$CREDIT_MANAGER_INIT" | base64 -w 0 | python3 -c "import sys,json; print(json.dumps(sys.stdin.read()))"),
    "yield_distributor_init_msg": $(echo -n "$YIELD_DISTRIBUTOR_INIT" | base64 -w 0 | python3 -c "import sys,json; print(json.dumps(sys.stdin.read()))"),
    "yield_reserve_init_msg": $(echo -n "$YIELD_RESERVE_INIT" | base64 -w 0 | python3 -c "import sys,json; print(json.dumps(sys.stdin.read()))")
  }
}
EOF
)

cp_tx=$(zigchaind tx wasm execute "${POOL_FACTORY_ADDRESS}" \
  "${CREATE_POOL_MSG}" \
  --from "${WALLET}" \
  --node "${NODE}" \
  --chain-id "${CHAIN_ID}" \
  ${GAS_FLAGS} \
  --output json \
  -y) || fail "CreatePool execute failed"

cp_txhash=$(echo "$cp_tx" | jq -r '.txhash')
info "Waiting for CreatePool tx ${cp_txhash}..."
sleep 10

# ─── 5. Query PoolFactory registry to confirm pool registration ───────────────
info "Querying PoolFactory registry to confirm pool registration..."
sleep 2

POOL_RECORD=$(zigchaind query wasm contract-state smart "${POOL_FACTORY_ADDRESS}" \
  '{"pool": {"pool_id": 0}}' \
  --node "${NODE}" \
  --output json) || fail "PoolFactory registry query failed"

POOL_RECORD_DATA=$(echo "$POOL_RECORD" | jq -r '.data')
[[ -z "$POOL_RECORD_DATA" || "$POOL_RECORD_DATA" == "null" ]] \
  && fail "Pool 0 was not registered in PoolFactory. CreatePool may have failed."

PSP_POOL_ADDRESS=$(echo     "$POOL_RECORD_DATA" | jq -r '.pool.psp_pool'         // "")
CREDIT_MGR_ADDRESS=$(echo   "$POOL_RECORD_DATA" | jq -r '.pool.credit_manager'   // "")
YIELD_DIST_ADDRESS=$(echo   "$POOL_RECORD_DATA" | jq -r '.pool.yield_distributor'// "")
YIELD_RES_ADDRESS=$(echo    "$POOL_RECORD_DATA" | jq -r '.pool.yield_reserve'    // "")

success "Pool 0 registered in PoolFactory registry ✓"
echo ""

# ─── 6. Write all addresses to scripts/vault_addresses.txt ───────────────────
OUTPUT_FILE="${SCRIPT_DIR}/vault_addresses.txt"

cat > "${OUTPUT_FILE}" <<EOF
# DeFa v2 deployment outputs
# Generated: $(date -u '+%Y-%m-%dT%H:%M:%SZ')
# Chain:     ${CHAIN_ID}
# Node:      ${NODE}

export NODE="${NODE}"
export CHAIN_ID="${CHAIN_ID}"
export WALLET="${WALLET}"
export ADMIN_ADDR="${ADMIN_ADDR}"
export GAS_PRICES="${GAS_PRICES}"
export GAS_ADJUSTMENT="${GAS_ADJUSTMENT}"

# Code IDs
export POOL_FACTORY_CODE_ID="${CODE_IDS[defa_pool_factory]}"
export PSP_POOL_CODE_ID="${CODE_IDS[defa_psp_pool]}"
export CREDIT_MANAGER_CODE_ID="${CODE_IDS[defa_credit_manager]}"
export YIELD_DISTRIBUTOR_CODE_ID="${CODE_IDS[defa_yield_distributor]}"
export YIELD_RESERVE_CODE_ID="${CODE_IDS[defa_yield_reserve]}"

# Contract addresses
export POOL_FACTORY_ADDRESS="${POOL_FACTORY_ADDRESS}"
export PSP_POOL_ADDRESS="${PSP_POOL_ADDRESS}"
export CREDIT_MANAGER_ADDRESS="${CREDIT_MGR_ADDRESS}"
export YIELD_DISTRIBUTOR_ADDRESS="${YIELD_DIST_ADDRESS}"
export YIELD_RESERVE_ADDRESS="${YIELD_RES_ADDRESS}"
EOF

# ─── Summary ──────────────────────────────────────────────────────────────────
echo ""
echo -e "${GREEN}══════════════════════════════════════${NC}"
echo -e "${GREEN}  DeFa v2 Deploy Complete ✓${NC}"
echo -e "${GREEN}══════════════════════════════════════${NC}"
echo ""
echo -e "${BLUE}Code IDs:${NC}"
printf "  %-30s %s\n" "defa-pool-factory:"      "${CODE_IDS[defa_pool_factory]}"
printf "  %-30s %s\n" "defa-psp-pool:"          "${CODE_IDS[defa_psp_pool]}"
printf "  %-30s %s\n" "defa-credit-manager:"    "${CODE_IDS[defa_credit_manager]}"
printf "  %-30s %s\n" "defa-yield-distributor:" "${CODE_IDS[defa_yield_distributor]}"
printf "  %-30s %s\n" "defa-yield-reserve:"     "${CODE_IDS[defa_yield_reserve]}"
echo ""
echo -e "${BLUE}Addresses:${NC}"
printf "  %-30s %s\n" "PoolFactory:"      "${POOL_FACTORY_ADDRESS}"
printf "  %-30s %s\n" "PSPPool:"          "${PSP_POOL_ADDRESS}"
printf "  %-30s %s\n" "CreditManager:"   "${CREDIT_MGR_ADDRESS}"
printf "  %-30s %s\n" "YieldDistributor:" "${YIELD_DIST_ADDRESS}"
printf "  %-30s %s\n" "YieldReserve:"    "${YIELD_RES_ADDRESS}"
echo ""
success "All addresses saved to: ${OUTPUT_FILE}"
echo -e "${YELLOW}Next:${NC} source scripts/vault_addresses.txt to use addresses in subsequent commands."
