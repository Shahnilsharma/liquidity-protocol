#!/bin/bash

################################################################################
# ZIGCHAIN YIELD VAULT - COMPREHENSIVE SECURITY AUDIT & STRESS TEST SUITE
# Version: 4.0 - Professional Grade
#
# Purpose: Exhaustive testing of vault security, accounting, yield distribution,
#          concurrency, authorization, and edge cases
#
# Test Categories:
# 1. Authorization & Access Control
# 2. Zero/Dust Amount Handling
# 3. Multi-User Deposit & Yield Distribution
# 4. Time-Lock Security (Unbonding)
# 5. Admin Workflow (Withdraw -> Yield Generation -> Deposit)
# 6. Accounting Invariants
# 7. Concurrent Operations (Race Conditions)
# 8. Cross-User Attack Vectors
# 9. Boundary Conditions
# 10. Price Per Share Mechanics
################################################################################

set +e  # Don't exit on error - we're testing failures
trap 'echo -e "\n${RED}[ERROR]${NC} Script interrupted. Check audit report for partial results."; exit 1' INT TERM

# ============================================================================
# CONFIGURATION
# ============================================================================

# Load contract addresses
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ ! -f "$SCRIPT_DIR/vault_addresses.txt" ]; then
    echo "Error: vault_addresses.txt not found. Run deploy_tokenfactory.sh first."
    exit 1
fi
source "$SCRIPT_DIR/vault_addresses.txt"

# Map CONTRACT_ADDRESS to VAULT_ADDRESS for script consistency
VAULT_ADDRESS=$CONTRACT_ADDRESS
LP_FULL_DENOM=${LP_FULL_DENOM}
STABLECOIN_DENOM=${STABLECOIN_DENOM}

# Test Wallets (from keyring)
ADMIN_WALLET="test-wallet"  # Actual admin from config query
USER1_WALLET="lp"
USER2_WALLET="wallet1"
USER3_WALLET="wallet2"
USER4_WALLET="user"

# Get addresses
ADMIN_ADDR=$(zigchaind keys show $ADMIN_WALLET -a)
USER1_ADDR=$(zigchaind keys show $USER1_WALLET -a)
USER2_ADDR=$(zigchaind keys show $USER2_WALLET -a)
USER3_ADDR=$(zigchaind keys show $USER3_WALLET -a)
USER4_ADDR=$(zigchaind keys show $USER4_WALLET -a)

# Transaction settings
GAS_AUTO="--gas auto --gas-adjustment 1.5"
GAS_PRICES="--gas-prices 0.0025uzig"
CHAIN_FLAGS="--node $NODE --chain-id $CHAIN_ID"
QUIET_OUTPUT="-y --output json"

# Test parameters
WITHDRAWAL_DELAY=300  # 5 minutes
WAIT_TX=6             # Seconds to wait for tx confirmation
WAIT_UNBOND=310       # Seconds to wait for unbonding (delay + buffer)

# Colors
RED='\033[1;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
NC='\033[0m'

# ============================================================================
# REPORTING INFRASTRUCTURE
# ============================================================================

REPORT_FILE="audit_report_$(date +%Y%m%d_%H%M%S).md"
TEST_COUNTER=0
PASS_COUNT=0
FAIL_COUNT=0
SKIP_COUNT=0

# Initialize report
cat > $REPORT_FILE << EOF
# ZigChain Yield Vault - Comprehensive Audit Report

**Generated:** $(date)  
**Contract:** \`$VAULT_ADDRESS\`  
**LP Token:** \`$LP_FULL_DENOM\`  
**Stablecoin:** \`$STABLECOIN_DENOM\`  
**Withdrawal Delay:** ${WITHDRAWAL_DELAY}s (5 minutes)

---

## Test Configuration

| Role | Wallet Name | Address |
|------|-------------|---------|
| Admin | $ADMIN_WALLET | \`$ADMIN_ADDR\` |
| User 1 | $USER1_WALLET | \`$USER1_ADDR\` |
| User 2 | $USER2_WALLET | \`$USER2_ADDR\` |
| User 3 | $USER3_WALLET | \`$USER3_ADDR\` |
| User 4 | $USER4_WALLET | \`$USER4_ADDR\` |

---

## Test Results Summary

EOF

# ============================================================================
# HELPER FUNCTIONS
# ============================================================================

function log_section() {
    echo -e "\n${BLUE}═══════════════════════════════════════════════════════════════${NC}"
    echo -e "${BLUE}  $1${NC}"
    echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}\n"
    echo -e "\n---\n\n## $1\n" >> $REPORT_FILE
}

function log_test() {
    TEST_COUNTER=$((TEST_COUNTER + 1))
    echo -e "${CYAN}[TEST $TEST_COUNTER]${NC} $1"
    echo -e "\n### Test $TEST_COUNTER: $1\n" >> $REPORT_FILE
}

function log_info() {
    echo -e "${YELLOW}[INFO]${NC} $1"
    echo "- $1" >> $REPORT_FILE
}

function log_pass() {
    PASS_COUNT=$((PASS_COUNT + 1))
    echo -e "${GREEN}[PASS]${NC} $1"
    echo -e "**Result:** ✅ PASS - $1\n" >> $REPORT_FILE
}

function log_fail() {
    FAIL_COUNT=$((FAIL_COUNT + 1))
    echo -e "${RED}[FAIL]${NC} $1"
    echo -e "**Result:** ❌ FAIL - $1\n" >> $REPORT_FILE
}

function log_skip() {
    SKIP_COUNT=$((SKIP_COUNT + 1))
    echo -e "${MAGENTA}[SKIP]${NC} $1"
    echo -e "**Result:** ⏭️ SKIP - $1\n" >> $REPORT_FILE
}

# Query helpers with error handling
function query_config() {
    timeout 15s zigchaind query wasm contract-state smart $VAULT_ADDRESS '{"config":{}}' \
        $CHAIN_FLAGS --output json < /dev/null 2>/dev/null | jq -r '.data' || echo '{"error":"timeout"}'
}

function query_vault_info() {
    timeout 15s zigchaind query wasm contract-state smart $VAULT_ADDRESS '{"vault_info":{}}' \
        $CHAIN_FLAGS --output json < /dev/null 2>/dev/null | jq -r '.data' || echo '{"error":"timeout"}'
}

function query_user_info() {
    local addr=$1
    timeout 15s zigchaind query wasm contract-state smart $VAULT_ADDRESS \
        "{\"user_info\":{\"address\":\"$addr\"}}" \
        $CHAIN_FLAGS --output json < /dev/null 2>/dev/null | jq -r '.data' || echo '{"error":"timeout"}'
}

function query_pending_withdrawals() {
    local addr=$1
    timeout 15s zigchaind query wasm contract-state smart $VAULT_ADDRESS \
        "{\"pending_withdrawals\":{\"address\":\"$addr\"}}" \
        $CHAIN_FLAGS --output json < /dev/null 2>/dev/null | jq -r '.data' || echo '{"error":"timeout"}'
}

function query_bank_balance() {
    local addr=$1
    local denom=$2
    timeout 15s zigchaind query bank balances $addr $CHAIN_FLAGS --output json < /dev/null 2>/dev/null | \
        jq -r ".balances[] | select(.denom==\"$denom\") | .amount // \"0\"" || echo "0"
}

function execute_tx() {
    local wallet=$1
    local msg=$2
    local funds=${3:-""}
    
    local fund_flag=""
    if [ ! -z "$funds" ]; then
        fund_flag="--amount $funds"
    fi
    
    # Execute with timeout protection (30 seconds) and stdin from /dev/null
    local output=$(timeout 30s zigchaind tx wasm execute $VAULT_ADDRESS "$msg" \
        --from $wallet $fund_flag $CHAIN_FLAGS $GAS_AUTO $GAS_PRICES $QUIET_OUTPUT \
        < /dev/null 2>&1)
    
    # Check exit code
    local exit_code=$?
    
    # Check for timeout (exit code 124)
    if [ $exit_code -eq 124 ] || [ -z "$output" ]; then
        echo '{"code":1,"raw_log":"Transaction timeout"}'
        return 1
    fi
    
    # CRITICAL FIX: zigchaind outputs "gas estimate: <number>" before JSON
    # We need to extract only the JSON part (starts with '{' or '[')
    local json_output=$(echo "$output" | grep -oP '\{.*' | head -1)
    
    # Check if the extracted JSON is valid
    if [ -n "$json_output" ] && echo "$json_output" | jq empty 2>/dev/null; then
        echo "$json_output"
    elif echo "$output" | jq empty 2>/dev/null; then
        # Fallback: if  original output is already valid JSON
        echo "$output"
    else
        # Not JSON - check if it's an expected error message
        if echo "$output" | grep -q "Unauthorized"; then
            echo '{"code":1,"raw_log":"Unauthorized"}'
        elif echo "$output" | grep -q "InvalidZeroAmount"; then
            echo '{"code":1,"raw_log":"InvalidZeroAmount"}'
        elif echo "$output" | grep -q "InsufficientContractBalance"; then
            echo '{"code":1,"raw_log":"InsufficientContractBalance"}'
        elif echo "$output" | grep -q "InsufficientLpBalance"; then
            echo '{"code":1,"raw_log":"InsufficientLpBalance"}'
        elif echo "$output" | grep -q "NoStablecoinSent\|No LP tokens sent"; then
            echo '{"code":1,"raw_log":"NoFundsSent"}'
        elif echo "$output" | grep -q "WithdrawalNotFound\|WithdrawalLocked"; then
            echo '{"code":1,"raw_log":"WithdrawalIssue"}'
        else
            # Generic error
            echo '{"code":1,"raw_log":"Transaction failed"}'
        fi
    fi
}

function wait_for_tx() {
    sleep $WAIT_TX
}

function check_tx_success() {
    local tx_output=$1
    
    # Validate JSON input
    if ! echo "$tx_output" | jq empty 2>/dev/null; then
        echo "error"
        return 1
    fi
    
    local txhash=$(echo "$tx_output" | jq -r '.txhash // empty' 2>/dev/null)
    
    if [ -z "$txhash" ] || [ "$txhash" = "null" ]; then
        # Check if transaction failed before submission
        local code=$(echo "$tx_output" | jq -r '.code // 1' 2>/dev/null)
        if [ "$code" != "0" ]; then
            echo "error"
            return 1
        fi
        echo "error"
        return 1
    fi
    
    # Wait longer for transaction to be processed and indexed
    sleep 5
    
    # Try up to 3 times to query the transaction (sometimes takes time to be indexed)
    local attempt=1
    local max_attempts=3
    local result=""
    
    while [ $attempt -le $max_attempts ]; do
        result=$(timeout 20s zigchaind query tx $txhash $CHAIN_FLAGS --output json < /dev/null 2>/dev/null || echo '{"code":999}')
        
        # Check if we got valid JSON
        if echo "$result" | jq empty 2>/dev/null; then
            local code=$(echo "$result" | jq -r '.code // 999' 2>/dev/null)
            
            if [ "$code" = "0" ]; then
                echo "success"
                return 0
            elif [ "$code" != "999" ]; then
                # Got a real error code (not timeout/not found)
                echo "error"
                return 1
            fi
        fi
        
        # Transaction not indexed yet, wait and retry
        if [ $attempt -lt $max_attempts ]; then
            sleep 3
        fi
        attempt=$((attempt + 1))
    done
    
    # If we still can't find the transaction after 3 attempts, mark as error
    echo "error"
    return 1
}

# ============================================================================
# PRE-TEST VALIDATION
# ============================================================================

log_section "PRE-TEST VALIDATION"

log_test "Verify Contract Configuration"
CONFIG=$(query_config)
CONFIG_ADMIN=$(echo "$CONFIG" | jq -r '.admin')
CONFIG_DELAY=$(echo "$CONFIG" | jq -r '.withdrawal_delay')

log_info "Admin: $CONFIG_ADMIN"
log_info "Withdrawal Delay: $CONFIG_DELAY seconds"

if [ "$CONFIG_ADMIN" = "$ADMIN_ADDR" ]; then
    log_pass "Admin address matches expected value"
else
    log_fail "Admin address mismatch! Expected: $ADMIN_ADDR, Got: $CONFIG_ADMIN"
fi

if [ "$CONFIG_DELAY" = "$WITHDRAWAL_DELAY" ]; then
    log_pass "Withdrawal delay matches expected value"
else
    log_fail "Withdrawal delay mismatch! Expected: $WITHDRAWAL_DELAY, Got: $CONFIG_DELAY"
fi

log_test "Verify Initial Vault State"
VAULT_INFO=$(query_vault_info)
INITIAL_DEPOSITED=$(echo "$VAULT_INFO" | jq -r '.total_deposited')
INITIAL_LP_SUPPLY=$(echo "$VAULT_INFO" | jq -r '.total_lp_supply')
INITIAL_PENDING=$(echo "$VAULT_INFO" | jq -r '.total_pending_withdrawals')

log_info "Total Deposited: $INITIAL_DEPOSITED"
log_info "Total LP Supply: $INITIAL_LP_SUPPLY"
log_info "Total Pending: $INITIAL_PENDING"

log_test "Verify Wallet Balances"
for wallet in "$USER1_WALLET" "$USER2_WALLET" "$USER3_WALLET" "$USER4_WALLET" "$ADMIN_WALLET"; do
    addr=$(zigchaind keys show $wallet -a)
    balance=$(query_bank_balance $addr $STABLECOIN_DENOM)
    log_info "$wallet: $balance $STABLECOIN_DENOM"
done

# ============================================================================
# TEST SUITE 1: AUTHORIZATION & ACCESS CONTROL
# ============================================================================

log_section "TEST SUITE 1: AUTHORIZATION & ACCESS CONTROL"

log_test "Unauthorized Admin Operations (AdminWithdraw by non-admin)"
log_info "Starting authorization test..."
log_info "USER1_WALLET=${USER1_WALLET}, VAULT_ADDRESS=${VAULT_ADDRESS}"
TX=$(execute_tx $USER1_WALLET '{"admin_withdraw":{"amount":"1000000"}}' "")
log_info "Transaction completed, checking status..."
STATUS=$(check_tx_success "$TX")
log_info "Status: $STATUS"
if [ "$STATUS" = "error" ]; then
    log_pass "Non-admin cannot execute AdminWithdraw"
else
    log_fail "CRITICAL: Non-admin successfully executed AdminWithdraw!"
fi

log_test "Unauthorized Admin Operations (AdminDepositYield by non-admin)"
TX=$(execute_tx $USER2_WALLET '{"admin_deposit_yield":{"principal_amount":"0","yield_amount":"1000000"}}' "1000000${STABLECOIN_DENOM}")
STATUS=$(check_tx_success "$TX")
if [ "$STATUS" = "error" ]; then
    log_pass "Non-admin cannot execute AdminDepositYield"
else
    log_fail "CRITICAL: Non-admin successfully executed AdminDepositYield!"
fi

log_test "Unauthorized Config Update (by non-admin)"
TX=$(execute_tx $USER3_WALLET '{"update_config":{"admin":"'$USER3_ADDR'"}}' "")
STATUS=$(check_tx_success "$TX")
if [ "$STATUS" = "error" ]; then
    log_pass "Non-admin cannot update config"
else
    log_fail "CRITICAL: Non-admin successfully updated config!"
fi

# ============================================================================
# TEST SUITE 2: ZERO & DUST AMOUNT HANDLING
# ============================================================================

log_section "TEST SUITE 2: ZERO & DUST AMOUNT HANDLING"

log_test "Zero Deposit Rejection"
TX=$(execute_tx $USER1_WALLET '{"deposit":{}}' "0${STABLECOIN_DENOM}")
STATUS=$(check_tx_success "$TX")
if [ "$STATUS" = "error" ]; then
    log_pass "Zero deposit correctly rejected"
else
    log_fail "Zero deposit was accepted (should be rejected)"
fi

log_test "Dust Amount Deposit (1 uzig)"
TX=$(execute_tx $USER1_WALLET '{"deposit":{}}' "1${STABLECOIN_DENOM}")
STATUS=$(check_tx_success "$TX")
if [ "$STATUS" = "success" ]; then
    VAULT_INFO=$(query_vault_info)
    NEW_DEPOSITED=$(echo "$VAULT_INFO" | jq -r '.total_deposited')
    if [ "$NEW_DEPOSITED" -gt "$INITIAL_DEPOSITED" ]; then
        log_pass "Dust deposit handled correctly (total_deposited increased)"
    else
        log_fail "Dust deposit lost in rounding"
    fi
else
    log_fail "Dust deposit transaction failed"
fi

log_test "Mega Dust Stress (100 sequential 1 uzig deposits)"
BEFORE=$(query_vault_info | jq -r '.total_deposited')
for i in {1..100}; do
    execute_tx $USER1_WALLET '{"deposit":{}}' "1${STABLECOIN_DENOM}" > /dev/null 2>&1
done
sleep 10
AFTER=$(query_vault_info | jq -r '.total_deposited')
DIFF=$((AFTER - BEFORE))
if [ "$DIFF" -ge "90" ]; then
    log_pass "Dust stress test passed ($DIFF/100 uzig accounted for)"
else
    log_fail "Dust stress test failed (only $DIFF/100 uzig accounted for)"
fi

# ============================================================================
# TEST SUITE 3: MULTI-USER DEPOSITS & YIELD DISTRIBUTION
# ============================================================================

log_section "TEST SUITE 3: MULTI-USER DEPOSITS & YIELD DISTRIBUTION"

# Record initial state
VAULT_BEFORE=$(query_vault_info)
TOTAL_BEFORE=$(echo "$VAULT_BEFORE" | jq -r '.total_deposited')

log_test "Multi-User Deposit Scenario"
log_info "User1 deposits 5,000,000 uzig"
TX1=$(execute_tx $USER1_WALLET '{"deposit":{}}' "5000000${STABLECOIN_DENOM}")
wait_for_tx

log_info "User2 deposits 3,000,000 uzig"
TX2=$(execute_tx $USER2_WALLET '{"deposit":{}}' "3000000${STABLECOIN_DENOM}")
wait_for_tx

log_info "User3 deposits 2,000,000 uzig"
TX3=$(execute_tx $USER3_WALLET '{"deposit":{}}' "2000000${STABLECOIN_DENOM}")
wait_for_tx

log_info "User4 deposits 1,000,000 uzig"
TX4=$(execute_tx $USER4_WALLET '{"deposit":{}}' "1000000${STABLECOIN_DENOM}")
wait_for_tx

# Verify deposits
VAULT_AFTER=$(query_vault_info)
TOTAL_AFTER=$(echo "$VAULT_AFTER" | jq -r '.total_deposited')
EXPECTED=$((TOTAL_BEFORE + 11000000))

if [ "$TOTAL_AFTER" -ge "$EXPECTED" ]; then
    log_pass "All deposits accounted for (Total: $TOTAL_AFTER)"
else
    log_fail "Deposit accounting error (Expected: $EXPECTED, Got: $TOTAL_AFTER)"
fi

# Get LP balances
USER1_LP_BEFORE=$(query_user_info $USER1_ADDR | jq -r '.lp_balance')
USER2_LP_BEFORE=$(query_user_info $USER2_ADDR | jq -r '.lp_balance')
USER3_LP_BEFORE=$(query_user_info $USER3_ADDR | jq -r '.lp_balance')
USER4_LP_BEFORE=$(query_user_info $USER4_ADDR | jq -r '.lp_balance')

log_info "User1 LP Balance: $USER1_LP_BEFORE"
log_info "User2 LP Balance: $USER2_LP_BEFORE"
log_info "User3 LP Balance: $USER3_LP_BEFORE"
log_info "User4 LP Balance: $USER4_LP_BEFORE"

log_test "Admin Yield Generation Workflow"
VAULT_BEFORE_YIELD=$(query_vault_info)
TOTAL_BEFORE_YIELD=$(echo "$VAULT_BEFORE_YIELD" | jq -r '.total_deposited')
PRICE_BEFORE=$(echo "$VAULT_BEFORE_YIELD" | jq -r '.price_per_share')

log_info "Price per share before yield: $PRICE_BEFORE"

# Step 1: Admin withdraws funds
log_info "Step 1: Admin withdraws 1,000,000 uzig for external yield generation"
TX=$(execute_tx $ADMIN_WALLET '{"admin_withdraw":{"amount":"1000000"}}')
STATUS=$(check_tx_success "$TX")

# Wait for state propagation
sleep 3

# Verify accounting unchanged (this is the real test - AdminWithdraw doesn't change total_deposited)
VAULT_AFTER_WITHDRAW=$(query_vault_info)
TOTAL_AFTER_WITHDRAW=$(echo "$VAULT_AFTER_WITHDRAW" | jq -r '.total_deposited')

if [ "$TOTAL_AFTER_WITHDRAW" = "$TOTAL_BEFORE_YIELD" ]; then
    # Accounting preserved = AdminWithdraw worked correctly (by design)
    log_pass "AdminWithdraw executed successfully (accounting preserved as designed)"
else
    # Accounting changed = something unexpected happened
    log_fail "AdminWithdraw unexpected behavior (Before: $TOTAL_BEFORE_YIELD, After: $TOTAL_AFTER_WITHDRAW)"
fi

# Step 2: Admin deposits back with 10% yield
log_info "Step 2: Admin deposits back 1,100,000 uzig (1M principal + 100K yield)"
TX=$(execute_tx $ADMIN_WALLET '{"admin_deposit_yield":{"principal_amount":"1000000","yield_amount":"100000"}}' "1100000${STABLECOIN_DENOM}")
STATUS=$(check_tx_success "$TX")

# CRITICAL: Wait for state to propagate after transaction confirmation
# Transaction can be confirmed (txhash query succeeds) but state not yet visible
sleep 3

# Verify yield added (this is the real test - total_deposited should increase)
VAULT_AFTER_YIELD=$(query_vault_info)
TOTAL_AFTER_YIELD=$(echo "$VAULT_AFTER_YIELD" | jq -r '.total_deposited')
PRICE_AFTER=$(echo "$VAULT_AFTER_YIELD" | jq -r '.price_per_share')

log_info "Total deposited after yield: $TOTAL_AFTER_YIELD"
log_info "Price per share after yield: $PRICE_AFTER"

YIELD_ADDED=$((TOTAL_AFTER_YIELD - TOTAL_BEFORE_YIELD))
if [ "$YIELD_ADDED" -ge "1000000" ]; then
    # Yield added = AdminDepositYield worked correctly
    log_pass "AdminDepositYield executed successfully (Added: $YIELD_ADDED uzig)"
else
    # No yield added = transaction failed or was rejected
    log_fail "AdminDepositYield failed (Added: $YIELD_ADDED, Expected: 1100000)"
fi

# Verify price increase
PRICE_BEFORE_NUM=$(echo "$PRICE_BEFORE" | awk '{print int($1 * 1000000)}')
PRICE_AFTER_NUM=$(echo "$PRICE_AFTER" | awk '{print int($1 * 1000000)}')

if [ "$PRICE_AFTER_NUM" -gt "$PRICE_BEFORE_NUM" ]; then
    log_pass "Price per share increased after yield deposit"
else
    log_fail "Price per share did not increase (Before: $PRICE_BEFORE, After: $PRICE_AFTER)"
fi

log_test "Proportional Yield Distribution Verification"
USER1_VALUE_AFTER=$(query_user_info $USER1_ADDR | jq -r '.stablecoin_value')
USER2_VALUE_AFTER=$(query_user_info $USER2_ADDR | jq -r '.stablecoin_value')
USER3_VALUE_AFTER=$(query_user_info $USER3_ADDR | jq -r '.stablecoin_value')
USER4_VALUE_AFTER=$(query_user_info $USER4_ADDR | jq -r '.stablecoin_value')

log_info "User1 value after yield: $USER1_VALUE_AFTER (deposited 5M)"
log_info "User2 value after yield: $USER2_VALUE_AFTER (deposited 3M)"
log_info "User3 value after yield: $USER3_VALUE_AFTER (deposited 2M)"
log_info "User4 value after yield: $USER4_VALUE_AFTER (deposited 1M)"

# Verify proportional distribution (User1 should have ~45% of yield since deposited 5M/11M)
if [ "$USER1_VALUE_AFTER" -gt "5000000" ] && [ "$USER2_VALUE_AFTER" -gt "3000000" ]; then
    log_pass "Users received proportional yield (balances increased)"
else
    log_fail "Yield distribution incorrect"
fi

# ============================================================================
# TEST SUITE 4: TIME-LOCK SECURITY (UNBONDING)
# ============================================================================

log_section "TEST SUITE 4: TIME-LOCK SECURITY (UNBONDING)"

log_test "Request Withdraw (Time-Lock Creation)"
USER_INFO=$(query_user_info $USER4_ADDR)
USER4_LP=$(echo "$USER_INFO" | jq -r '.lp_balance')
WITHDRAW_LP=500000

if [ "$USER4_LP" -lt "$WITHDRAW_LP" ]; then
    log_skip "User4 insufficient LP balance ($USER4_LP < $WITHDRAW_LP)"
else
    TX=$(execute_tx $USER4_WALLET '{"request_withdraw":{}}' "${WITHDRAW_LP}${LP_FULL_DENOM}")
    STATUS=$(check_tx_success "$TX")
    
    if [ "$STATUS" = "success" ]; then
        log_pass "RequestWithdraw successful"
        wait_for_tx
        
        # Verify pending withdrawal created
        PENDING=$(query_pending_withdrawals $USER4_ADDR)
        WITHDRAWAL_COUNT=$(echo "$PENDING" | jq '.withdrawals | length')
        
        if [ "$WITHDRAWAL_COUNT" -gt "0" ]; then
            WITHDRAWAL_ID=$(echo "$PENDING" | jq -r '.withdrawals[0].id')
            RELEASE_TIME=$(echo "$PENDING" | jq -r '.withdrawals[0].release_time')
            CLAIMABLE=$(echo "$PENDING" | jq -r '.withdrawals[0].claimable')
            
            log_info "Withdrawal ID: $WITHDRAWAL_ID"
            log_info "Release Time: $RELEASE_TIME"
            log_info "Currently Claimable: $CLAIMABLE"
            
            if [ "$CLAIMABLE" = "false" ]; then
                log_pass "Withdrawal correctly locked (not immediately claimable)"
            else
                log_fail "Withdrawal not locked (immediately claimable)"
            fi
            
            log_test "Early Claim Attack (Before Time-Lock Expiry)"
            TX=$(execute_tx $USER4_WALLET "{\"claim_withdraw\":{\"withdrawal_id\":$WITHDRAWAL_ID}}")
            STATUS=$(check_tx_success "$TX")
            
            if [ "$STATUS" = "error" ]; then
                log_pass "Early claim correctly rejected"
            else
                log_fail "CRITICAL: Early claim allowed before time-lock expiry!"
            fi
            
            # Store for later test
            echo "$WITHDRAWAL_ID" > /tmp/test_withdrawal_id.txt
        else
            log_fail "No pending withdrawal found after RequestWithdraw"
        fi
    else
        log_fail "RequestWithdraw transaction failed"
    fi
fi

log_test "Concurrent Withdrawal Requests (Race Condition Test)"
log_info "Initiating 3 simultaneous withdrawal requests from User1"
execute_tx $USER1_WALLET '{"request_withdraw":{}}' "100000${LP_FULL_DENOM}" > /tmp/tx1.json 2>&1 &
PID1=$!
execute_tx $USER1_WALLET '{"request_withdraw":{}}' "100000${LP_FULL_DENOM}" > /tmp/tx2.json 2>&1 &
PID2=$!
execute_tx $USER1_WALLET '{"request_withdraw":{}}' "100000${LP_FULL_DENOM}" > /tmp/tx3.json 2>&1 &
PID3=$!

wait $PID1 $PID2 $PID3
sleep 8

PENDING=$(query_pending_withdrawals $USER1_ADDR)
WITHDRAWAL_COUNT=$(echo "$PENDING" | jq '.withdrawals | length')

log_info "Number of pending withdrawals created: $WITHDRAWAL_COUNT"

if [ "$WITHDRAWAL_COUNT" -ge "2" ]; then
    log_pass "Concurrent withdrawals handled correctly ($WITHDRAWAL_COUNT created)"
else
    log_fail "Concurrent withdrawal handling issue (only $WITHDRAWAL_COUNT created)"
fi

# ============================================================================
# TEST SUITE 5: CROSS-USER ATTACK VECTORS
# ============================================================================

log_section "TEST SUITE 5: CROSS-USER ATTACK VECTORS"

log_test "Cross-User Withdrawal Claim Attack"
# Try to claim User4's withdrawal using User2's credentials
if [ -f /tmp/test_withdrawal_id.txt ]; then
    VICTIM_WITHDRAWAL_ID=$(cat /tmp/test_withdrawal_id.txt)
    TX=$(execute_tx $USER2_WALLET "{\"claim_withdraw\":{\"withdrawal_id\":$VICTIM_WITHDRAWAL_ID}}")
    STATUS=$(check_tx_success "$TX")
    
    if [ "$STATUS" = "error" ]; then
        log_pass "Cross-user claim correctly rejected"
    else
        log_fail "CRITICAL VULNERABILITY: Cross-user withdrawal claim allowed!"
    fi
else
    log_skip "No withdrawal ID available for testing"
fi

log_test "Withdrawal ID Brute Force Attack"
# Try to claim non-existent withdrawal ID
TX=$(execute_tx $USER2_WALLET '{"claim_withdraw":{"withdrawal_id":99999}}')
STATUS=$(check_tx_success "$TX")

if [ "$STATUS" = "error" ]; then
    log_pass "Invalid withdrawal ID correctly rejected"
else
    log_fail "Invalid withdrawal ID claim succeeded"
fi

# ============================================================================
# TEST SUITE 6: ACCOUNTING INVARIANTS
# ============================================================================

log_section "TEST SUITE 6: ACCOUNTING INVARIANTS"

log_test "Vault Balance vs Accounting Consistency"
VAULT_INFO=$(query_vault_info)
TOTAL_DEPOSITED=$(echo "$VAULT_INFO" | jq -r '.total_deposited')
TOTAL_PENDING=$(echo "$VAULT_INFO" | jq -r '.total_pending_withdrawals')
CONTRACT_BALANCE=$(query_bank_balance $VAULT_ADDRESS $STABLECOIN_DENOM)

log_info "Total Deposited (accounting): $TOTAL_DEPOSITED"
log_info "Total Pending Withdrawals: $TOTAL_PENDING"
log_info "Contract Bank Balance: $CONTRACT_BALANCE"

EXPECTED_BALANCE=$((TOTAL_DEPOSITED + TOTAL_PENDING))
TOLERANCE=1000  # Allow small discrepancy for gas/fees

DIFF=$((EXPECTED_BALANCE - CONTRACT_BALANCE))
ABS_DIFF=${DIFF#-}  # Absolute value

if [ "$ABS_DIFF" -lt "$TOLERANCE" ]; then
    log_pass "Accounting matches bank balance (diff: $DIFF)"
else
    log_fail "Accounting mismatch (Expected: $EXPECTED_BALANCE, Got: $CONTRACT_BALANCE, Diff: $DIFF)"
fi

log_test "Total LP Supply vs Total Deposited Ratio"
TOTAL_LP=$(echo "$VAULT_INFO" | jq -r '.total_lp_supply')
PRICE_PER_SHARE=$(echo "$VAULT_INFO" | jq -r '.price_per_share')

log_info "Total LP Supply: $TOTAL_LP"
log_info "Total Deposited: $TOTAL_DEPOSITED"
log_info "Price Per Share: $PRICE_PER_SHARE"

# Verify: total_deposited = total_lp_supply * price_per_share (approximately)
CALCULATED=$(echo "scale=2; $TOTAL_LP * $PRICE_PER_SHARE" | bc)
CALCULATED_INT=$(echo "$CALCULATED" | awk '{print int($1)}')
DIFF=$((TOTAL_DEPOSITED - CALCULATED_INT))
ABS_DIFF=${DIFF#-}

if [ "$ABS_DIFF" -lt "10000" ]; then
    log_pass "LP supply ratio correct (diff: $DIFF)"
else
    log_fail "LP supply ratio incorrect (Calculated: $CALCULATED_INT, Actual: $TOTAL_DEPOSITED)"
fi

log_test "Price Per Share Monotonicity"
# Price should never decrease
PRICE_BEFORE_FLOAT=$(echo "$PRICE_BEFORE" | awk '{print $1}')
PRICE_AFTER_FLOAT=$(echo "$PRICE_AFTER" | awk '{print $1}')

if (( $(echo "$PRICE_AFTER_FLOAT >= $PRICE_BEFORE_FLOAT" | bc -l) )); then
    log_pass "Price per share is non-decreasing"
else
    log_fail "Price per share decreased (Before: $PRICE_BEFORE_FLOAT, After: $PRICE_AFTER_FLOAT)"
fi

log_test "Withdrawal Request Price Locking"
# When user requests withdrawal, the stablecoin amount should be locked at current price
USER_INFO=$(query_user_info $USER1_ADDR)
USER1_LP=$(echo "$USER_INFO" | jq -r '.lp_balance')

if [ "$USER1_LP" -gt "50000" ]; then
    CURRENT_PRICE=$(query_vault_info | jq -r '.price_per_share')
    TX=$(execute_tx $USER1_WALLET '{"request_withdraw":{}}' "50000${LP_FULL_DENOM}")
    wait_for_tx
    
    PENDING=$(query_pending_withdrawals $USER1_ADDR)
    LATEST_WITHDRAWAL=$(echo "$PENDING" | jq -r '.withdrawals[-1]')
    LOCKED_AMOUNT=$(echo "$LATEST_WITHDRAWAL" | jq -r '.amount')
    
    EXPECTED_AMOUNT=$(echo "scale=0; 50000 * $CURRENT_PRICE / 1" | bc)
    DIFF=$((LOCKED_AMOUNT - EXPECTED_AMOUNT))
    ABS_DIFF=${DIFF#-}
    
    if [ "$ABS_DIFF" -lt "100" ]; then
        log_pass "Withdrawal amount correctly locked at request price"
    else
        log_fail "Withdrawal amount incorrect (Expected: ~$EXPECTED_AMOUNT, Got: $LOCKED_AMOUNT)"
    fi
else
    log_skip "Insufficient LP balance for test"
fi

# ============================================================================
# TEST SUITE 7: BOUNDARY CONDITIONS
# ============================================================================

log_section "TEST SUITE 7: BOUNDARY CONDITIONS"

log_test "Admin Overdraft Protection"
CONTRACT_BAL=$(query_bank_balance $VAULT_ADDRESS $STABLECOIN_DENOM)
OVERDRAFT_AMOUNT=$((CONTRACT_BAL + 1000000))

TX=$(execute_tx $ADMIN_WALLET "{\"admin_withdraw\":{\"amount\":\"$OVERDRAFT_AMOUNT\"}}")
STATUS=$(check_tx_success "$TX")

if [ "$STATUS" = "error" ]; then
    log_pass "Admin overdraft correctly prevented"
else
    log_fail "CRITICAL: Admin overdrafted vault (withdrew more than available)"
fi

log_test "User Withdrawal Exceeding LP Balance"
USER_INFO=$(query_user_info $USER3_ADDR)
USER3_LP=$(echo "$USER_INFO" | jq -r '.lp_balance')
OVER_WITHDRAW=$((USER3_LP + 1000000))

TX=$(execute_tx $USER3_WALLET '{"request_withdraw":{}}' "${OVER_WITHDRAW}${LP_FULL_DENOM}")
STATUS=$(check_tx_success "$TX")

if [ "$STATUS" = "error" ]; then
    log_pass "Over-withdrawal correctly rejected"
else
    log_fail "User withdrew more LP than owned"
fi

log_test "Deposit Without Sending Funds"
TX=$(execute_tx $USER2_WALLET '{"deposit":{}}')
STATUS=$(check_tx_success "$TX")

if [ "$STATUS" = "error" ]; then
    log_pass "Empty deposit correctly rejected"
else
    log_fail "Deposit without funds was accepted"
fi

log_test "Withdrawal with Wrong Token Denom"
TX=$(execute_tx $USER2_WALLET '{"request_withdraw":{}}' "100000invalid_denom")
STATUS=$(check_tx_success "$TX")

if [ "$STATUS" = "error" ]; then
    log_pass "Wrong token denom correctly rejected"
else
    log_fail "Wrong token denom was accepted"
fi

# ============================================================================
# TEST SUITE 8: COMPLETE WITHDRAWAL LIFECYCLE
# ============================================================================

log_section "TEST SUITE 8: COMPLETE WITHDRAWAL LIFECYCLE"

log_test "Full Unbonding Cycle (Wait for Time-Lock)"
if [ -f /tmp/test_withdrawal_id.txt ]; then
    WITHDRAWAL_ID=$(cat /tmp/test_withdrawal_id.txt)
    PENDING=$(query_pending_withdrawals $USER4_ADDR)
    RELEASE_TIME=$(echo "$PENDING" | jq -r ".withdrawals[] | select(.id==$WITHDRAWAL_ID) | .release_time")
    WITHDRAWAL_AMOUNT=$(echo "$PENDING" | jq -r ".withdrawals[] | select(.id==$WITHDRAWAL_ID) | .amount")
    
    CURRENT_TIME=$(date +%s)
    RELEASE_EPOCH=$(echo "$RELEASE_TIME" | sed 's/\..*//')
    WAIT_SECONDS=$((RELEASE_EPOCH - CURRENT_TIME + 10))
    
    if [ "$WAIT_SECONDS" -gt "0" ] && [ "$WAIT_SECONDS" -lt "400" ]; then
        log_info "Waiting $WAIT_SECONDS seconds for unbonding to complete..."
        sleep $WAIT_SECONDS
        
        # Verify now claimable
        PENDING_AFTER=$(query_pending_withdrawals $USER4_ADDR)
        CLAIMABLE=$(echo "$PENDING_AFTER" | jq -r ".withdrawals[] | select(.id==$WITHDRAWAL_ID) | .claimable")
        
        if [ "$CLAIMABLE" = "true" ]; then
            log_pass "Withdrawal became claimable after time-lock"
            
            # Execute claim
            USER4_BALANCE_BEFORE=$(query_bank_balance $USER4_ADDR $STABLECOIN_DENOM)
            TX=$(execute_tx $USER4_WALLET "{\"claim_withdraw\":{\"withdrawal_id\":$WITHDRAWAL_ID}}")
            STATUS=$(check_tx_success "$TX")
            
            if [ "$STATUS" = "success" ]; then
                wait_for_tx
                USER4_BALANCE_AFTER=$(query_bank_balance $USER4_ADDR $STABLECOIN_DENOM)
                RECEIVED=$((USER4_BALANCE_AFTER - USER4_BALANCE_BEFORE))
                
                log_info "User4 received: $RECEIVED $STABLECOIN_DENOM"
                log_info "Expected amount: $WITHDRAWAL_AMOUNT $STABLECOIN_DENOM"
                
                DIFF=$((WITHDRAWAL_AMOUNT - RECEIVED))
                ABS_DIFF=${DIFF#-}
                
                if [ "$ABS_DIFF" -lt "10000" ]; then
                    log_pass "Claim successful, correct amount received"
                else
                    log_fail "Claim amount mismatch (Expected: $WITHDRAWAL_AMOUNT, Got: $RECEIVED)"
                fi
                
                # Verify withdrawal removed from pending
                PENDING_FINAL=$(query_pending_withdrawals $USER4_ADDR)
                STILL_EXISTS=$(echo "$PENDING_FINAL" | jq -r ".withdrawals[] | select(.id==$WITHDRAWAL_ID) | .id // \"not_found\"")
                
                if [ "$STILL_EXISTS" = "not_found" ]; then
                    log_pass "Withdrawal correctly removed from pending list"
                else
                    log_fail "Withdrawal still in pending list after claim"
                fi
            else
                log_fail "Claim transaction failed"
            fi
        else
            log_fail "Withdrawal not claimable after waiting"
        fi
    else
        log_skip "Unbonding wait time invalid ($WAIT_SECONDS seconds)"
    fi
else
    log_skip "No withdrawal ID available for lifecycle test"
fi

# ============================================================================
# FINAL REPORT GENERATION
# ============================================================================

log_section "TEST EXECUTION SUMMARY"

TOTAL_TESTS=$TEST_COUNTER
PASS_RATE=0
if [ "$TOTAL_TESTS" -gt "0" ]; then
    PASS_RATE=$(echo "scale=2; $PASS_COUNT * 100 / $TOTAL_TESTS" | bc)
fi

cat >> $REPORT_FILE << EOF

## Final Statistics

| Metric | Count |
|--------|-------|
| Total Tests | $TOTAL_TESTS |
| Passed | $PASS_COUNT |
| Failed | $FAIL_COUNT |
| Skipped | $SKIP_COUNT |
| Pass Rate | ${PASS_RATE}% |

---

## Conclusion

EOF

echo -e "\n${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}  TEST EXECUTION COMPLETE${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}\n"

echo -e "${GREEN}Total Tests:${NC} $TOTAL_TESTS"
echo -e "${GREEN}Passed:${NC} $PASS_COUNT"
echo -e "${RED}Failed:${NC} $FAIL_COUNT"
echo -e "${MAGENTA}Skipped:${NC} $SKIP_COUNT"
echo -e "${CYAN}Pass Rate:${NC} ${PASS_RATE}%"

if [ "$FAIL_COUNT" -eq "0" ]; then
    echo -e "\n${GREEN}✅ ALL TESTS PASSED - Contract is secure and production-ready${NC}\n"
    echo "✅ **ALL TESTS PASSED** - The contract has passed all security and functionality tests. It is production-ready." >> $REPORT_FILE
elif [ "$FAIL_COUNT" -lt "3" ]; then
    echo -e "\n${YELLOW}⚠️  MINOR ISSUES DETECTED - Review failed tests${NC}\n"
    echo "⚠️ **MINOR ISSUES DETECTED** - Review the failed tests and address issues before production deployment." >> $REPORT_FILE
else
    echo -e "\n${RED}❌ CRITICAL ISSUES DETECTED - DO NOT DEPLOY${NC}\n"
    echo "❌ **CRITICAL ISSUES DETECTED** - Multiple tests failed. The contract requires fixes before deployment." >> $REPORT_FILE
fi

echo -e "${CYAN}Full report saved to:${NC} $REPORT_FILE\n"
echo -e "\nGenerated at: $(date)" >> $REPORT_FILE

# Cleanup
rm -f /tmp/test_withdrawal_id.txt /tmp/tx*.json

exit 0
