#!/bin/bash
# =============================================================================
#  ZigChain Vault Automation Daemon
#  Version: 1.0.0
#
#  PURPOSE:
#    Runs indefinitely to automate the yield-generation lifecycle for the
#    Admin-Managed Yield Vault deployed on ZigChain testnet.
#
#  LIFECYCLE IT AUTOMATES:
#    1. DEPOSIT DETECTED   → AdminWithdraw from vault → Delegate to validator
#    2. WITHDRAW REQUESTED → Ensure vault has liquidity or Unbond from validator
#    3. UNBONDING COMPLETE → AdminDepositYield (principal returned, no yield yet)
#    4. REWARD HARVEST     → Collect staking rewards → AdminDepositYield as yield
#    5. CLAIMABLE CHECK    → Log when users can claim (vault must have balance)
#
#  DESIGN NOTES:
#    • The contract's total_deposited does NOT change when admin withdraws.
#      It only increases on user deposits and yield injections.
#    • admin_withdraw(amount)  → vault sends uzig to admin (accounting unchanged)
#    • admin_deposit_yield(principal, yield) → admin sends principal+yield back;
#      only yield is added to total_deposited (price per share increases).
#    • TESTNET: staking unbonding period = 168h (7 days) = vault withdrawal lock.
#      Because timing is equal, unbonding can theoretically cover every request,
#      BUT a LIQUIDITY_BUFFER_PCT is still kept liquid to:
#        – Cover multiple simultaneous withdrawal requests
#        – Absorb slight on-chain timing variance (block time jitter)
#        – Pay gas from the admin wallet without running dry
#      On mainnet where unbonding > vault lock, raise this buffer significantly.
#
#  USAGE:
#    ./scripts/vault_daemon.sh [OPTIONS]
#    Options:
#      --dry-run   Print what would happen without sending transactions
#      --once      Run one cycle then exit (useful for cron or testing)
#      --status    Print current vault + staking state and exit
#      --help      Show this message
#
#  REQUIREMENTS:
#    • zigchaind in PATH
#    • jq in PATH
#    • Admin key (ADMIN_KEY) loaded in zigchaind keyring
#    • bc for arithmetic
# =============================================================================

set -eo pipefail

# =============================================================================
#  DEPLOYMENT CONFIGURATION
#  ─────────────────────────────────────────────────────────────────────────────
#  These are the ONLY variables you need to change when switching environments
#  (e.g. testnet → mainnet or deploying a new vault contract).
#
#  To migrate to mainnet:
#    1. Set NODE / LCD_NODE / CHAIN_ID to mainnet values
#    2. Set VAULT_CONTRACT to the new deployed contract address
#    3. Set LP_FULL_DENOM to match (coin.<new_contract>.<subdenom>)
#    4. Set ADMIN_KEY / ADMIN_ADDR to the mainnet admin key
#    5. Set VALIDATOR_ADDR to your chosen mainnet validator
#    6. Raise LIQUIDITY_BUFFER_PCT (mainnet unbonding > vault lock)
# =============================================================================

# ── Network ──────────────────────────────────────────────────────────────────
NODE="https://public-zigchain-testnet-rpc.numia.xyz:443"   # Testnet RPC
#NODE="https://rpc.zigchain.io:443"                         # Mainnet RPC (example)

LCD_NODE="https://public-zigchain-testnet-lcd.numia.xyz"   # Testnet LCD (REST)
#LCD_NODE="https://lcd.zigchain.io"                         # Mainnet LCD (example)

CHAIN_ID="zig-test-2"          # Testnet chain ID
#CHAIN_ID="zigchain-1"          # Mainnet chain ID (example)

# ── Contract ─────────────────────────────────────────────────────────────────
# The vault smart contract deployed on chain (from keyscon.md instantiation)
VAULT_CONTRACT="zig1l7kpzmynguy09ln4ar8tsrna7v09uzxrk27440aw7l72reaz3hgs0mqr4j"

# Native stablecoin denom accepted by the vault
STABLECOIN_DENOM="uzig"

# Full LP token denom minted by the vault  (coin.<contract_addr>.<subdenom>)
# Must match: coin.${VAULT_CONTRACT}.<subdenom_used_at_instantiation>
LP_FULL_DENOM="coin.${VAULT_CONTRACT}.tikiter"

# ── Admin wallet ─────────────────────────────────────────────────────────────
# Key name as it appears in `zigchaind keys list`
ADMIN_KEY="newaddmin"
# Bech32 address of the admin key (used for on-chain balance / delegation queries)
ADMIN_ADDR="zig1pvucnrgua60k4kdzzawcq4qnx0pgq4zu7zdzdd"

# ── Validator ────────────────────────────────────────────────────────────────
# Validator operator address to stake deposited funds with
VALIDATOR_ADDR="zigvaloper1pwwymlyeyfcz3pjvcegvz8tj3yf0pr3wqqhrwk"

# ── Gas ──────────────────────────────────────────────────────────────────────
GAS_PRICES="0.0025uzig"   # Adjust if network congestion changes base fee
GAS_ADJUSTMENT="1.5"      # Multiplier on simulated gas estimate

# =============================================================================
#  DAEMON TUNING
#  ─────────────────────────────────────────────────────────────────────────────
#  These control how aggressively / frequently the daemon acts.
#  Defaults are sensible for testnet; review before going to mainnet.
# =============================================================================

# How often to poll the vault contract for new deposits / withdrawal requests
POLL_INTERVAL=30              # seconds  (testnet: 30s  |  mainnet suggestion: 60s)

# How often to harvest staking rewards and inject them as yield
REWARD_COLLECT_INTERVAL=3600  # seconds  (1 hour — rewards accumulate slowly)

# Seconds to wait after broadcasting a TX before querying its result
TX_CONFIRM_WAIT=8             # testnet blocks fast; increase to 12-15 on mainnet

# How many times to retry a failed transaction before giving up
TX_MAX_RETRIES=3

# Percentage of total_deposited to keep liquid (un-staked) in the vault as a
# withdrawal buffer.
#
#   TESTNET  : unbonding = 7 days = vault withdrawal lock  → 10% is enough
#   MAINNET  : if unbonding > vault lock, raise this to 25–30% so short-term
#              withdrawals are always covered without waiting for un-bonding.
#
LIQUIDITY_BUFFER_PCT=10       # % of total_deposited kept liquid (not staked)

# Minimum amounts to avoid sending dust transactions
MIN_STAKE_AMOUNT=1000000      # 1 ZIG  (1_000_000 uzig) — don't stake less than this
MIN_UNBOND_AMOUNT=100000      # 0.1 ZIG (100_000 uzig)  — unbond any amount needed for user safety
MIN_REWARD_AMOUNT=500000      # 0.5 ZIG — don't collect rewards below this

# ── Paths (auto-derived, do not change) ──────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
STATE_FILE="${SCRIPT_DIR}/.vault_daemon_state.json"
LOG_FILE="${SCRIPT_DIR}/vault_daemon.log"
PID_FILE="${SCRIPT_DIR}/.vault_daemon.pid"

# ─────────────────────────────── COLOUR CODES ────────────────────────────────

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
BOLD='\033[1m'
NC='\033[0m'

# ─────────────────────────────── FLAGS ───────────────────────────────────────

DRY_RUN=false
RUN_ONCE=false
STATUS_ONLY=false

for arg in "$@"; do
    case "$arg" in
        --dry-run)  DRY_RUN=true  ;;
        --once)     RUN_ONCE=true ;;
        --status)   STATUS_ONLY=true ;;
        --help)
            sed -n '3,30p' "$0" | sed 's/^#//'
            exit 0
            ;;
    esac
done

# ─────────────────────────────── LOGGING ─────────────────────────────────────

log() {
    local level="$1"; shift
    local msg="$*"
    local ts
    ts="$(date '+%Y-%m-%d %H:%M:%S')"
    local colour=""
    case "$level" in
        INFO)    colour="$GREEN"   ;;
        WARN)    colour="$YELLOW"  ;;
        ERROR)   colour="$RED"     ;;
        ACTION)  colour="$CYAN"    ;;
        STATE)   colour="$MAGENTA" ;;
        HEADER)  colour="$BOLD$BLUE" ;;
        TXHASH)  colour="$BOLD$GREEN" ;;
        *)       colour="$NC"       ;;
    esac
    local line="[$ts] [$level] $msg"
    # Always write to log file
    echo "$line" >> "$LOG_FILE"
    # Only echo to terminal when running interactively (stderr is a TTY).
    # When launched as a background daemon (nohup ... >> log 2>&1) stderr is NOT
    # a TTY, so we skip the echo — prevents every line appearing twice in the log.
    if [[ -t 2 ]]; then
        echo -e "${colour}${line}${NC}" >&2
    fi
}

log_header() { log HEADER "══════════════════════════════════════════════════"; log HEADER "$*"; log HEADER "══════════════════════════════════════════════════"; }
log_info()   { log INFO  "$*"; }
log_warn()   { log WARN  "$*"; }
log_error()  { log ERROR "$*"; }
log_action() { log ACTION "$*"; }
log_state()  { log STATE  "$*"; }
log_tx()     { log TXHASH "TX ✓ $*"; }
# ─────────────────────────────── PREREQUISITE CHECKS ─────────────────────────

check_prerequisites() {
    local missing=()
    command -v zigchaind &>/dev/null || missing+=("zigchaind")
    command -v jq        &>/dev/null || missing+=("jq")
    command -v bc        &>/dev/null || missing+=("bc")
    if [[ ${#missing[@]} -gt 0 ]]; then
        log_error "Missing required tools: ${missing[*]}"
        exit 1
    fi

    # Verify admin key is accessible
    local addr
    addr=$(zigchaind keys show "$ADMIN_KEY" -a 2>/dev/null) || {
        log_error "Admin key '$ADMIN_KEY' not found in keyring. Make sure 'newaddmin' is loaded."
        exit 1
    }
    if [[ "$addr" != "$ADMIN_ADDR" ]]; then
        log_warn "Admin key address '$addr' does not match expected '$ADMIN_ADDR'. Continuing..."
        ADMIN_ADDR="$addr"
    fi
    log_info "Admin address verified: $ADMIN_ADDR"
}

# ─────────────────────────────── STATE FILE ──────────────────────────────────
# The state file keeps daemon-side memory between cycles so we detect deltas.
# Schema:
#   last_total_deposited          – last seen vault total_deposited (string, uzig)
#   last_total_pending            – last seen vault total_pending_withdrawals (string)
#   last_reward_collection_epoch  – unix epoch of last reward harvest
#   pending_unbondings            – JSON array of {principal, initiated_epoch}
#   staked_principal              – total uzig currently delegated by admin (string)

init_state() {
    if [[ ! -f "$STATE_FILE" ]]; then
        log_info "Initialising state file at $STATE_FILE"
        cat > "$STATE_FILE" <<'EOF'
{
  "last_total_deposited": "0",
  "last_total_pending": "0",
  "last_reward_collection_epoch": 0,
  "pending_unbondings": [],
  "staked_principal": "0"
}
EOF
    fi
}

state_get() { jq -r ".$1 // empty" "$STATE_FILE" 2>/dev/null; }

state_set() {
    local key="$1" val="$2"
    local tmp
    tmp=$(mktemp)
    jq --arg k "$key" --arg v "$val" '.[$k] = $v' "$STATE_FILE" > "$tmp" && mv "$tmp" "$STATE_FILE"
}

state_set_num() {
    local key="$1" val="$2"
    local tmp
    tmp=$(mktemp)
    jq --arg k "$key" --argjson v "$val" '.[$k] = $v' "$STATE_FILE" > "$tmp" && mv "$tmp" "$STATE_FILE"
}

# Add an unbonding record to the pending_unbondings array
state_add_unbonding() {
    local principal="$1"
    local epoch
    epoch=$(date +%s)
    local tmp
    tmp=$(mktemp)
    jq --argjson p "$principal" --argjson e "$epoch" \
        '.pending_unbondings += [{"principal": $p, "initiated_epoch": $e}]' \
        "$STATE_FILE" > "$tmp" && mv "$tmp" "$STATE_FILE"
}

# Remove unbondings older than a threshold (i.e., they have likely completed).
# Sets global COMPLETED_UNBONDINGS_JSON with the completed entries array.
# Updates the state file to remove those entries.
state_pop_completed_unbondings() {
    local unbonding_secs="${1:-604800}"  # testnet default: 7 days = 604800s
    local now
    now=$(date +%s)
    local cutoff=$(( now - unbonding_secs ))

    COMPLETED_UNBONDINGS_JSON=$(jq --argjson c "$cutoff" \
        '[.pending_unbondings[] | select(.initiated_epoch <= $c)]' \
        "$STATE_FILE" 2>/dev/null || echo "[]")

    local kept_json tmpfile
    kept_json=$(jq --argjson c "$cutoff" \
        '[.pending_unbondings[] | select(.initiated_epoch > $c)]' \
        "$STATE_FILE" 2>/dev/null || echo "[]")

    tmpfile=$(mktemp)
    jq --argjson kept "$kept_json" '.pending_unbondings = $kept' \
        "$STATE_FILE" > "$tmpfile" && mv "$tmpfile" "$STATE_FILE"
}

# ─────────────────────────────── LOCK (single instance) ──────────────────────

acquire_lock() {
    if [[ -f "$PID_FILE" ]]; then
        local old_pid
        old_pid=$(cat "$PID_FILE")
        if kill -0 "$old_pid" 2>/dev/null; then
            log_error "Daemon already running (PID $old_pid). Exiting."
            exit 1
        else
            log_warn "Stale PID file found (PID $old_pid is dead). Removing."
            rm -f "$PID_FILE"
        fi
    fi
    echo $$ > "$PID_FILE"
}

release_lock() { rm -f "$PID_FILE"; }

# ─────────────────────────────── ZigChain HELPERS ────────────────────────────

# Execute a command with retries, return exit code
run_with_retry() {
    local retries=0
    local max="$TX_MAX_RETRIES"
    until "$@"; do
        retries=$(( retries + 1 ))
        if [[ $retries -ge $max ]]; then
            log_error "Command failed after $max attempts: $*"
            return 1
        fi
        log_warn "Attempt $retries/$max failed, retrying in 5s..."
        sleep 5
    done
}

# Submit a tx and wait for confirmation; echoes txhash on success
submit_tx() {
    local desc="$1"; shift
    local cmd=("$@")

    if [[ "$DRY_RUN" == "true" ]]; then
        log_action "[DRY-RUN] WOULD EXECUTE: ${cmd[*]}"
        echo "DRY_RUN_TX"
        return 0
    fi

    log_action "Submitting TX: $desc"
    local raw
    # Capture stdout only for JSON parsing; redirect stderr separately so that
    # zigchaind's "gas estimate: XXXXX" line (which goes to stderr) never
    # contaminates the JSON and breaks jq.
    local tx_stderr
    raw=$("${cmd[@]}" --output json -y 2>/tmp/_vd_tx_stderr) || {
        tx_stderr=$(cat /tmp/_vd_tx_stderr 2>/dev/null)
        log_error "TX submission failed: $tx_stderr"
        return 1
    }
    tx_stderr=$(cat /tmp/_vd_tx_stderr 2>/dev/null)
    [[ -n "$tx_stderr" ]] && log_info "TX info: $tx_stderr"

    local txhash
    txhash=$(echo "$raw" | jq -r '.txhash // empty')
    if [[ -z "$txhash" ]]; then
        log_error "No txhash in response: $raw"
        return 1
    fi

    log_info "TX broadcast → $txhash (waiting ${TX_CONFIRM_WAIT}s)"
    sleep "$TX_CONFIRM_WAIT"

    local result code
    result=$(zigchaind query tx "$txhash" --node "$NODE" --output json 2>/dev/null)
    code=$(echo "$result" | jq -r '.code // 1')

    if [[ "$code" == "0" ]]; then
        log_tx "$txhash"
        echo "$txhash"
        return 0
    else
        local raw_log
        raw_log=$(echo "$result" | jq -r '.raw_log // "unknown error"')
        log_error "TX failed (code=$code): $raw_log"
        return 1
    fi
}

# ─────────────────────────────── QUERY HELPERS ───────────────────────────────

# Query vault info; returns JSON object
query_vault_info() {
    zigchaind query wasm contract-state smart "$VAULT_CONTRACT" \
        '{"vault_info":{}}' \
        --node "$NODE" --output json 2>/dev/null | jq '.data'
}

query_vault_config() {
    zigchaind query wasm contract-state smart "$VAULT_CONTRACT" \
        '{"config":{}}' \
        --node "$NODE" --output json 2>/dev/null | jq '.data'
}

# Contract's bank balance (what's physically in the vault)
query_vault_bank_balance() {
    zigchaind query bank balances "$VAULT_CONTRACT" \
        --node "$NODE" --output json 2>/dev/null | \
        jq -r --arg d "$STABLECOIN_DENOM" \
        '(.balances // []) | map(select(.denom==$d)) | .[0].amount // "0"' 2>/dev/null || echo "0"
}

# Admin wallet balance
query_admin_balance() {
    zigchaind query bank balances "$ADMIN_ADDR" \
        --node "$NODE" --output json 2>/dev/null | \
        jq -r --arg d "$STABLECOIN_DENOM" \
        '(.balances // []) | map(select(.denom==$d)) | .[0].amount // "0"' 2>/dev/null || echo "0"
}

# Amount currently delegated to validator by admin
query_delegated_amount() {
    zigchaind query staking delegation "$ADMIN_ADDR" "$VALIDATOR_ADDR" \
        --node "$NODE" --output json 2>/dev/null | \
        jq -r '.delegation_response.balance.amount // "0"' 2>/dev/null || echo "0"
}

# All unbonding delegation entries for admin → returns JSON array of entries
query_unbonding_delegations() {
    zigchaind query staking unbonding-delegation "$ADMIN_ADDR" "$VALIDATOR_ADDR" \
        --node "$NODE" --output json 2>/dev/null | \
        jq -c '.unbond.entries // []' 2>/dev/null || echo "[]"
}

# Pending staking rewards (in uzig, truncated to integer)
query_pending_rewards() {
    local rewards_json amount
    rewards_json=$(zigchaind query distribution rewards "$ADMIN_ADDR" "$VALIDATOR_ADDR" \
        --node "$NODE" --output json 2>/dev/null)
    amount=$(echo "$rewards_json" | jq -r \
        --arg d "$STABLECOIN_DENOM" \
        '(.rewards // []) | map(select(.denom==$d)) | .[0].amount // "0"' 2>/dev/null | \
        awk -F'.' '{print ($1 == "" ? "0" : $1)}')
    echo "${amount:-0}"
}

# Get unbonding time from chain params.
# ZigChain testnet: 168h0m0s = 604800s (7 days) – same as vault withdrawal lock.
# Cosmos SDK returns duration as "168h0m0s" or plain "604800s" – parse either form.
query_unbonding_time() {
    local raw
    raw=$(zigchaind query staking params \
        --node "$NODE" --output json 2>/dev/null | \
        jq -r '.params.unbonding_time // ""' 2>/dev/null)

    if [[ -z "$raw" ]]; then
        echo "604800"  # testnet fallback: 7 days
        return
    fi

    # Handle plain number (possibly with trailing 's'): e.g. "604800s" or "604800"
    if [[ "$raw" =~ ^[0-9]+s?$ ]]; then
        echo "${raw//s/}"
        return
    fi

    # Parse Go duration string: e.g. "168h0m0s"
    local h m s total
    h=$(echo "$raw" | grep -oP '[0-9]+(?=h)' || echo 0)
    m=$(echo "$raw" | grep -oP '[0-9]+(?=m)' || echo 0)
    s=$(echo "$raw" | grep -oP '[0-9]+(?=s)' || echo 0)
    total=$(( ${h:-0}*3600 + ${m:-0}*60 + ${s:-0} ))
    if (( total == 0 )); then
        echo "604800"  # testnet fallback: 7 days
    else
        echo "$total"
    fi
}

# ─────────────────────────────── STATUS DISPLAY ──────────────────────────────

print_status() {
    log_header "VAULT DAEMON STATUS"

    local vault_info vault_balance admin_balance delegated pending_rewards
    vault_info=$(query_vault_info)
    vault_balance=$(query_vault_bank_balance | head -1)
    admin_balance=$(query_admin_balance)
    delegated=$(query_delegated_amount)
    pending_rewards=$(query_pending_rewards)

    local total_deposited total_lp pending_withdrawals price
    total_deposited=$(echo "$vault_info" | jq -r '.total_deposited // "0"')
    total_lp=$(echo "$vault_info" | jq -r '.total_lp_supply // "0"')
    pending_withdrawals=$(echo "$vault_info" | jq -r '.total_pending_withdrawals // "0"')
    price=$(echo "$vault_info" | jq -r '.price_per_share // "1.0"')

    log_state "── VAULT CONTRACT ──────────────────────────"
    log_state "  Address          : $VAULT_CONTRACT"
    log_state "  total_deposited  : $total_deposited $STABLECOIN_DENOM"
    log_state "  total_lp_supply  : $total_lp LP"
    log_state "  pending_withdraw : $pending_withdrawals $STABLECOIN_DENOM"
    log_state "  price_per_share  : $price"
    log_state "  vault bank bal   : $vault_balance $STABLECOIN_DENOM"
    log_state ""
    log_state "── ADMIN WALLET ─────────────────────────────"
    log_state "  Address          : $ADMIN_ADDR"
    log_state "  Balance          : $admin_balance $STABLECOIN_DENOM"
    log_state "  Staked           : $delegated $STABLECOIN_DENOM"
    log_state "  Pending rewards  : $pending_rewards $STABLECOIN_DENOM"
    log_state ""
    log_state "── LOCAL STATE ──────────────────────────────"
    log_state "  last_deposited   : $(state_get last_total_deposited)"
    log_state "  last_pending     : $(state_get last_total_pending)"
    log_state "  staked_principal : $(state_get staked_principal)"
    log_state "  last reward at   : $(date -d @"$(state_get last_reward_collection_epoch)" 2>/dev/null || echo 'never')"
    log_state "  pending unbondings:"
    jq -r '.pending_unbondings[] | "    amount=\(.principal) uzig, initiated=\(.initiated_epoch | todate)"' \
        "$STATE_FILE" 2>/dev/null || true
}

# ─────────────────────────────── CORE ACTION: STAKE ──────────────────────────

do_stake() {
    local amount="$1"

    if (( amount < MIN_STAKE_AMOUNT )); then
        log_info "Skipping stake: amount $amount uzig is below minimum $MIN_STAKE_AMOUNT"
        return 0
    fi

    log_action "STAKE: AdminWithdraw $amount uzig from vault → delegate to validator"

    # Step 1: AdminWithdraw from vault to admin wallet
    local txhash
    txhash=$(submit_tx "admin_withdraw $amount uzig" \
        zigchaind tx wasm execute "$VAULT_CONTRACT" \
            "{\"admin_withdraw\":{\"amount\":\"$amount\"}}" \
            --from "$ADMIN_KEY" \
            --node "$NODE" \
            --chain-id "$CHAIN_ID" \
            --gas auto --gas-adjustment "$GAS_ADJUSTMENT" \
            --gas-prices "$GAS_PRICES") || {
        log_error "AdminWithdraw failed – skipping stake this cycle"
        return 1
    }

    if [[ "$txhash" == "DRY_RUN_TX" ]]; then
        log_info "[DRY-RUN] Would delegate $amount uzig to $VALIDATOR_ADDR"
        return 0
    fi

    # Small buffer to ensure funds land in admin wallet before delegating
    sleep 3

    # Step 2: Delegate to validator
    local stake_txhash
    stake_txhash=$(submit_tx "delegate $amount uzig to $VALIDATOR_ADDR" \
        zigchaind tx staking delegate "$VALIDATOR_ADDR" "${amount}${STABLECOIN_DENOM}" \
            --from "$ADMIN_KEY" \
            --node "$NODE" \
            --chain-id "$CHAIN_ID" \
            --gas auto --gas-adjustment "$GAS_ADJUSTMENT" \
            --gas-prices "$GAS_PRICES") || {
        log_error "Delegation failed! Funds are in admin wallet. Manual recovery needed."
        return 1
    }

    # Update tracked staked_principal
    local cur_staked
    cur_staked=$(state_get staked_principal)
    cur_staked=$(( cur_staked + amount ))
    state_set staked_principal "$cur_staked"
    log_info "Staked $amount uzig. Total tracked staked: $cur_staked uzig"
}

# ─────────────────────────────── CORE ACTION: UNBOND ─────────────────────────

do_unbond() {
    local amount="$1"
    local reason="${2:-withdrawal_request}"

    if (( amount < MIN_UNBOND_AMOUNT )); then
        log_info "Skipping unbond: $amount uzig below minimum $MIN_UNBOND_AMOUNT"
        return 0
    fi

    # Don't unbond more than what's delegated
    local delegated
    delegated=$(query_delegated_amount)
    if (( amount > delegated )); then
        log_warn "Requested to unbond $amount but only $delegated is delegated. Capping."
        amount="$delegated"
    fi
    if (( amount == 0 )); then
        log_warn "Nothing delegated to unbond."
        return 0
    fi

    log_action "UNBOND: initiating unbond of $amount uzig from $VALIDATOR_ADDR (reason: $reason)"

    local txhash
    txhash=$(submit_tx "unbond $amount uzig ($reason)" \
        zigchaind tx staking unbond "$VALIDATOR_ADDR" "${amount}${STABLECOIN_DENOM}" \
            --from "$ADMIN_KEY" \
            --node "$NODE" \
            --chain-id "$CHAIN_ID" \
            --gas auto --gas-adjustment "$GAS_ADJUSTMENT" \
            --gas-prices "$GAS_PRICES") || {
        log_error "Unbond transaction failed for $amount uzig"
        return 1
    }

    if [[ "$txhash" != "DRY_RUN_TX" ]]; then
        state_add_unbonding "$amount"
        local cur_staked
        cur_staked=$(state_get staked_principal)
        cur_staked=$(( cur_staked - amount ))
        state_set staked_principal "$cur_staked"
        log_info "Unbonding started for $amount uzig. Tracked staked: $cur_staked"
    fi
}

# ─────────────────────────────── CORE ACTION: DEPOSIT YIELD ──────────────────

do_deposit_yield() {
    local principal="$1"
    local yield="$2"
    local total=$(( principal + yield ))

    if (( total == 0 )); then
        log_warn "do_deposit_yield called with total=0, skipping"
        return 0
    fi

    # Verify admin has enough balance
    if [[ "$DRY_RUN" != "true" ]]; then
        local admin_bal
        admin_bal=$(query_admin_balance)
        if (( admin_bal < total )); then
            log_error "Admin wallet has $admin_bal uzig but needs $total uzig for deposit_yield"
            return 1
        fi
    fi

    log_action "ADMIN_DEPOSIT_YIELD: principal=$principal yield=$yield total=$total uzig"

    local txhash
    txhash=$(submit_tx "admin_deposit_yield principal=$principal yield=$yield" \
        zigchaind tx wasm execute "$VAULT_CONTRACT" \
            "{\"admin_deposit_yield\":{\"principal_amount\":\"$principal\",\"yield_amount\":\"$yield\"}}" \
            --from "$ADMIN_KEY" \
            --amount "${total}${STABLECOIN_DENOM}" \
            --node "$NODE" \
            --chain-id "$CHAIN_ID" \
            --gas auto --gas-adjustment "$GAS_ADJUSTMENT" \
            --gas-prices "$GAS_PRICES") || {
        log_error "admin_deposit_yield failed. Principal=$principal yield=$yield"
        return 1
    }

    if [[ "$txhash" != "DRY_RUN_TX" ]]; then
        log_info "Yield deposited. LP price per share will increase by yield=$yield uzig."
    fi
}

# ─────────────────────────────── CORE ACTION: COLLECT REWARDS ────────────────

do_collect_rewards() {
    local pending_rewards
    pending_rewards=$(query_pending_rewards)

    if (( pending_rewards < MIN_REWARD_AMOUNT )); then
        log_info "Skipping reward collection: $pending_rewards uzig < min $MIN_REWARD_AMOUNT"
        # Still update epoch so interval timer resets (don't re-check every 30s)
        state_set last_reward_collection_epoch "$(date +%s)"
        return 0
    fi

    log_action "REWARDS: Collecting $pending_rewards uzig staking rewards"

    local txhash
    txhash=$(submit_tx "withdraw-rewards from $VALIDATOR_ADDR" \
        zigchaind tx distribution withdraw-rewards "$VALIDATOR_ADDR" \
            --from "$ADMIN_KEY" \
            --node "$NODE" \
            --chain-id "$CHAIN_ID" \
            --gas auto --gas-adjustment "$GAS_ADJUSTMENT" \
            --gas-prices "$GAS_PRICES") || {
        log_error "Reward collection failed"
        return 1
    }

    if [[ "$txhash" == "DRY_RUN_TX" ]]; then
        log_info "[DRY-RUN] Would deposit $pending_rewards uzig as yield"
        return 0
    fi

    # Wait for funds to land, then deposit as pure yield (principal=0)
    sleep 3

    # Use the pending_rewards minus gas estimate as the safe yield amount
    # Actually query actual balance increase – but simplest is (pending_rewards - small_gas_buffer)
    # Approximate: rewards minus 2x base gas cost
    local gas_buffer=10000
    local yield_to_deposit=$(( pending_rewards - gas_buffer ))
    if (( yield_to_deposit <= 0 )); then
        log_warn "Reward $pending_rewards uzig too small to deposit after gas. Skipping yield deposit."
        return 0
    fi

    do_deposit_yield "0" "$yield_to_deposit"

    # Record last collection time
    state_set_num last_reward_collection_epoch "$(date +%s)"
    log_info "Rewards harvested and deposited as yield. LP price per share increased."
}

# ─────────────────────────────── MONITORING CYCLE ────────────────────────────

run_cycle() {
    log_header "MONITORING CYCLE  [$(date '+%Y-%m-%d %H:%M:%S')]"

    # ── 1. Fetch current on-chain state ──────────────────────────────────────
    local vault_info vault_bank_bal delegated unbonding_entries
    vault_info=$(query_vault_info) || { log_error "Failed to query vault info"; return 1; }
    vault_bank_bal=$(query_vault_bank_balance | head -1)
    delegated=$(query_delegated_amount)

    local total_deposited total_lp total_pending
    total_deposited=$(echo "$vault_info" | jq -r '.total_deposited // "0"')
    total_lp=$(echo "$vault_info" | jq -r '.total_lp_supply // "0"')
    total_pending=$(echo "$vault_info" | jq -r '.total_pending_withdrawals // "0"')
    local price
    price=$(echo "$vault_info" | jq -r '.price_per_share // "1.0"')

    # Retrieve local state from last cycle
    local prev_total_deposited prev_total_pending
    prev_total_deposited=$(state_get last_total_deposited)
    prev_total_pending=$(state_get last_total_pending)
    prev_total_deposited="${prev_total_deposited:-0}"
    prev_total_pending="${prev_total_pending:-0}"

    log_state "Vault total_deposited=$total_deposited | lp=$total_lp | pending=$total_pending | price=$price"
    log_state "Vault bank balance=$vault_bank_bal | Admin delegated=$delegated"

    # ── 2. Handle completed unbondings ───────────────────────────────────────
    # Pop any unbondings we initiated that are likely complete (past unbonding period)
    local unbonding_time
    unbonding_time=$(query_unbonding_time)
    COMPLETED_UNBONDINGS_JSON="[]"
    state_pop_completed_unbondings "$unbonding_time"
    local num_completed
    num_completed=$(echo "$COMPLETED_UNBONDINGS_JSON" | jq 'length')

    if (( num_completed > 0 )); then
        log_info "Detected $num_completed completed unbonding(s). Depositing principal back to vault."
        local total_principal
        total_principal=$(echo "$COMPLETED_UNBONDINGS_JSON" | jq '[.[].principal] | add // 0')

        if (( total_principal > 0 )); then
            # Check if admin actually received the funds
            local admin_bal
            admin_bal=$(query_admin_balance)
            if (( admin_bal >= total_principal )); then
                # Deposit principal back (no yield, yield collected separately via reward harvest)
                do_deposit_yield "$total_principal" "0"
            else
                log_warn "Unbonding complete but admin balance ($admin_bal) < expected principal ($total_principal)."
                log_warn "Funds may still be arriving. Re-queueing for next cycle."
                # Re-add them with a fresh timestamp so they'll be rechecked in next round
                for i in $(seq 0 $(( num_completed - 1 ))); do
                    local p
                    p=$(echo "$COMPLETED_UNBONDINGS_JSON" | jq ".[$i].principal")
                    state_add_unbonding "$p"
                done
            fi
        fi
    fi

    # ── Pre-compute in-flight unbonding total ────────────────────────────────
    # Core accounting identity: total_deposited = vault_bank_bal + delegated + total_unbonding
    # In-flight unbondings WILL arrive to cover pending withdrawals, so we must not
    # double-reserve vault funds for pending that is already covered by unbondings.
    local total_unbonding
    total_unbonding=$(jq '[.pending_unbondings[].principal] | add // 0' "$STATE_FILE" 2>/dev/null || echo 0)
    # pending_needing_coverage = how much of total_pending is NOT yet covered by unbondings
    local _pc=$(( total_unbonding < total_pending ? total_unbonding : total_pending ))
    local pending_needing_coverage=$(( total_pending - _pc ))

    # ── 3. Detect new deposits & stake any vault surplus ─────────────────────
    local deposit_delta=$(( total_deposited - prev_total_deposited ))
    if (( deposit_delta > 0 )); then
        log_info "New deposits detected: +$deposit_delta uzig (total=$total_deposited)"
    fi

    # Always check for stakeable surplus every cycle.
    # This also catches cases where a prior cycle detected a deposit but couldn't stake
    # (e.g., because pending was overestimated without considering in-flight unbondings).
    local target_liquid=$(( total_deposited * LIQUIDITY_BUFFER_PCT / 100 ))
    local current_vault_bal="${vault_bank_bal:-0}"
    # Safe to stake = vault balance minus buffer minus pending NOT already covered by in-flight unbondings.
    local safe_to_stake=$(( current_vault_bal - target_liquid - pending_needing_coverage ))

    if (( safe_to_stake >= MIN_STAKE_AMOUNT )); then
        log_info "Staking $safe_to_stake uzig (vault=$current_vault_bal, buffer=$target_liquid, pending_uncovered=$pending_needing_coverage, unbonding_in_flight=$total_unbonding)"
        do_stake "$safe_to_stake" || log_warn "Staking failed this cycle, will retry next."
    fi

    # ── 4. Detect new withdrawal requests → ensure liquidity / unbond ────────
    local pending_delta=$(( total_pending - prev_total_pending ))

    if (( pending_delta > 0 )); then
        log_info "New withdrawal request(s) detected: +$pending_delta uzig pending (total=$total_pending)"

        # Re-fetch vault bank balance (may have changed after staking above)
        vault_bank_bal=$(query_vault_bank_balance | head -1)
        # Coverage = vault liquid funds + in-flight unbondings (which will arrive)
        local total_coverage=$(( vault_bank_bal + total_unbonding ))
        local shortfall=$(( total_pending - total_coverage ))

        if (( shortfall > 0 )); then
            log_warn "Insufficient coverage: vault=$vault_bank_bal + unbonding=$total_unbonding = $total_coverage < pending=$total_pending. Need to unbond $shortfall uzig."
            do_unbond "$shortfall" "withdrawal_request_coverage"
        else
            log_info "Pending withdrawals covered: vault=$vault_bank_bal + unbonding=$total_unbonding = $total_coverage >= pending=$total_pending"
        fi
    fi

    # ── 5. Safety check: ensure pending withdrawals are always covered ────────
    # Coverage = vault liquid + in-flight unbondings. Both count toward meeting pending.
    local current_vault_after
    current_vault_after=$(query_vault_bank_balance | head -1)
    local total_coverage_now=$(( current_vault_after + total_unbonding ))

    if (( total_pending > 0 && total_coverage_now < total_pending )); then
        local coverage_shortfall=$(( total_pending - total_coverage_now ))
        log_warn "SAFETY CHECK: vault=$current_vault_after + unbonding=$total_unbonding = $total_coverage_now < pending=$total_pending. Shortfall=$coverage_shortfall"

        # Always unbond the exact shortfall — existing unbondings are already included
        # in total_coverage_now, so if we're still short, we genuinely need more.
        log_warn "Initiating emergency unbond of $coverage_shortfall uzig to cover pending withdrawals."
        do_unbond "$coverage_shortfall" "emergency_coverage"
    fi

    # ── 6. Periodic reward collection ────────────────────────────────────────
    local last_reward_epoch
    last_reward_epoch=$(state_get last_reward_collection_epoch)
    last_reward_epoch="${last_reward_epoch:-0}"
    local now_epoch
    now_epoch=$(date +%s)
    local secs_since_rewards=$(( now_epoch - last_reward_epoch ))

    if (( secs_since_rewards >= REWARD_COLLECT_INTERVAL )); then
        log_info "Reward collection interval reached ($secs_since_rewards >= $REWARD_COLLECT_INTERVAL). Checking rewards."
        do_collect_rewards || log_warn "Reward collection failed, will retry in next interval."
    fi

    # ── 7. Reconcile orphaned admin balance → delegate directly ──────────────
    # Accounting identity: total_deposited = vault_bank_bal + delegated + total_unbonding + orphaned
    # Orphaned = funds that left the vault (admin_withdraw) but were never delegated and never returned.
    local orphaned_in_admin=$(( total_deposited - vault_bank_bal - delegated - total_unbonding ))
    if (( orphaned_in_admin < 0 )); then orphaned_in_admin=0; fi

    if (( orphaned_in_admin > MIN_STAKE_AMOUNT )); then
        local admin_bal_now
        admin_bal_now=$(query_admin_balance)
        if (( admin_bal_now >= orphaned_in_admin )); then
            log_warn "RECONCILE: orphaned=${orphaned_in_admin} uzig detected (total_deposited=$total_deposited - vault=$vault_bank_bal - delegated=$delegated - unbonding=$total_unbonding). Delegating directly."
            local _reconcile_hash
            if _reconcile_hash=$(submit_tx "reconcile-delegate ${orphaned_in_admin} uzig" \
                zigchaind tx staking delegate "$VALIDATOR_ADDR" "${orphaned_in_admin}${STABLECOIN_DENOM}" \
                    --from "$ADMIN_KEY" --node "$NODE" --chain-id "$CHAIN_ID" \
                    --gas auto --gas-adjustment "$GAS_ADJUSTMENT" --gas-prices "$GAS_PRICES"); then
                state_set staked_principal "$(( $(state_get staked_principal) + orphaned_in_admin ))"
            else
                log_warn "RECONCILE: Delegation failed, will retry next cycle."
            fi
        else
            log_warn "RECONCILE: Expected ${orphaned_in_admin} uzig orphaned but admin only has ${admin_bal_now}. Skipping (funds may still be in-flight)."
        fi
    fi

    # ── 8. Persist current state for next cycle ───────────────────────────────
    state_set last_total_deposited "$total_deposited"
    state_set last_total_pending   "$total_pending"

    log_info "Cycle complete. Next in ${POLL_INTERVAL}s."
}

# ─────────────────────────────── MAIN ────────────────────────────────────────

# Graceful shutdown on SIGINT / SIGTERM
cleanup() {
    log_info "Shutdown signal received. Releasing lock and exiting."
    release_lock
    exit 0
}
trap cleanup SIGINT SIGTERM

main() {
    log_header "ZigChain Vault Automation Daemon  v1.0.0"
    log_info "Network   : $CHAIN_ID  →  $NODE"
    log_info "Contract  : $VAULT_CONTRACT"
    log_info "Admin key : $ADMIN_KEY ($ADMIN_ADDR)"
    log_info "Validator : $VALIDATOR_ADDR"
    log_info "Buffer    : ${LIQUIDITY_BUFFER_PCT}% of vault kept liquid (testnet unbonding=7d = vault lock)"
    log_info "Poll      : every ${POLL_INTERVAL}s"
    log_info "Log file  : $LOG_FILE"
    if [[ "$DRY_RUN" == "true" ]]; then
        log_warn "DRY-RUN MODE: No transactions will be sent."
    fi

    check_prerequisites
    init_state

    if [[ "$STATUS_ONLY" == "true" ]]; then
        print_status
        exit 0
    fi

    if [[ "$RUN_ONCE" != "true" ]]; then
        acquire_lock
    fi

    if [[ "$RUN_ONCE" == "true" ]]; then
        run_cycle
    else
        # Infinite daemon loop
        while true; do
            run_cycle || log_error "Cycle encountered errors; will resume in ${POLL_INTERVAL}s."
            sleep "$POLL_INTERVAL"
        done
    fi

    release_lock
}

main "$@"
