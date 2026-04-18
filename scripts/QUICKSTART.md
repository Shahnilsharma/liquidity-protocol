# 🚀 Quick Start - Run Audit Now

## TL;DR - Run This Command

```bash
cd ~/Desktop_extracted/Desktop/NewFolder/lp-transfer/liquidity-protocol
./scripts/comprehensive_audit.sh
```

**⏱️ Duration:** 10-15 minutes  
**📊 Output:** Real-time colored console + detailed Markdown report

---

## Pre-Flight Checklist (60 seconds)

### ✅ 1. Verify Wallets Exist
```bash
for wallet in test-wallet lp wallet1 wallet2 user; do
    zigchaind keys show $wallet -a 2>/dev/null && echo "✅ $wallet" || echo "❌ $wallet MISSING"
done
```

**Expected:** All 5 wallets show ✅

**If any ❌:** You need to create/import that wallet first.

---

### ✅ 2. Check Wallet Balances
```bash
source scripts/vault_addresses.txt
for wallet in test-wallet lp wallet1 wallet2 user; do
    addr=$(zigchaind keys show $wallet -a)
    bal=$(zigchaind query bank balances $addr --node $NODE -o json | jq -r '.balances[] | select(.denom=="uzig") | .amount // "0"')
    echo "$wallet: $bal uzig"
done
```

**Minimum Required:**
- `test-wallet` (admin): 5,000,000 uzig
- `lp`: 10,000,000 uzig
- `wallet1`: 10,000,000 uzig
- `wallet2`: 5,000,000 uzig
- `user`: 5,000,000 uzig

**If insufficient:** Get tokens from faucet or send from another wallet

---

### ✅ 3. Verify Contract Deployed
```bash
source scripts/vault_addresses.txt
echo "Contract: $VAULT_ADDRESS"
zigchaind query wasm contract-state smart $VAULT_ADDRESS '{"config":{}}' --node $NODE -o json | jq -r '.data.admin'
```

**Expected:** Shows admin address (should be `test-wallet` address)

**If error:** Run `./scripts/deploy_v2_stack.sh` first

---

## Run The Audit

### Option 1: Full Audit (Recommended)
```bash
./scripts/comprehensive_audit.sh
```

Watch the color-coded output in real-time!

### Option 2: Run in Background and Monitor Log
```bash
./scripts/comprehensive_audit.sh 2>&1 | tee audit_run.log &
tail -f audit_run.log
```

### Option 3: Silent Mode (Report Only)
```bash
./scripts/comprehensive_audit.sh > /dev/null 2>&1
```

---

## What to Expect

### Phase 1: Pre-Test Validation (30 seconds)
```
═══════════════════════════════════════════════════════════════
  PRE-TEST VALIDATION
═══════════════════════════════════════════════════════════════

[TEST 1] Verify Contract Configuration
[INFO] Admin: zig1ug335mpcdn2vpk8p08v4k9z7cqtdg0jj4tqr92
[INFO] Withdrawal Delay: 300 seconds
[PASS] Admin address matches expected value
[PASS] Withdrawal delay matches expected value
```

### Phase 2: Security Tests (2-3 minutes)
```
═══════════════════════════════════════════════════════════════
  TEST SUITE 1: AUTHORIZATION & ACCESS CONTROL
═══════════════════════════════════════════════════════════════

[TEST 4] Unauthorized Admin Operations (AdminWithdraw by non-admin)
[PASS] Non-admin cannot execute AdminWithdraw
```

### Phase 3: Multi-User Testing (3-4 minutes)
```
═══════════════════════════════════════════════════════════════
  TEST SUITE 3: MULTI-USER DEPOSITS & YIELD DISTRIBUTION
═══════════════════════════════════════════════════════════════

[TEST 10] Multi-User Deposit Scenario
[INFO] User1 deposits 5,000,000 uzig
[INFO] User2 deposits 3,000,000 uzig
```

### Phase 4: Time-Lock Testing (5-6 minutes)
```
═══════════════════════════════════════════════════════════════
  TEST SUITE 8: COMPLETE WITHDRAWAL LIFECYCLE
═══════════════════════════════════════════════════════════════

[TEST 30] Full Unbonding Cycle (Wait for Time-Lock)
[INFO] Waiting 305 seconds for unbonding to complete...
```

**⏳ This is the longest part** - The script waits for actual 300s time-lock

### Phase 5: Final Summary (10 seconds)
```
═══════════════════════════════════════════════════════════════
  TEST EXECUTION COMPLETE
═══════════════════════════════════════════════════════════════

Total Tests: 30
Passed: 28
Failed: 2
Skipped: 0
Pass Rate: 93.33%

✅ ALL TESTS PASSED - Contract is secure and production-ready

Full report saved to: audit_report_20260214_153045.md
```

---

## Reading the Report

### Quick View - Check Pass Rate
```bash
ls -t audit_report_*.md | head -1 | xargs grep "Pass Rate"
```

**Expected:** `Pass Rate: 100%` or `95-100%`

### View Full Report
```bash
cat $(ls -t audit_report_*.md | head -1) | less
```

### Check for Critical Failures
```bash
grep -i "CRITICAL" $(ls -t audit_report_*.md | head -1)
```

**Expected:** No output (no critical failures)

### Count Test Results
```bash
REPORT=$(ls -t audit_report_*.md | head -1)
echo "Passed: $(grep -c 'Result.*PASS' $REPORT)"
echo "Failed: $(grep -c 'Result.*FAIL' $REPORT)"
echo "Skipped: $(grep -c 'Result.*SKIP' $REPORT)"
```

---

## Understanding Results

### ✅ Perfect Score (100% Pass)
```
Total Tests: 30
Passed: 30
Failed: 0
Pass Rate: 100%

✅ ALL TESTS PASSED - Contract is secure and production-ready
```

**Action:** You're good to go! Contract is production-ready.

---

### ⚠️ Minor Issues (90-99% Pass)
```
Total Tests: 30
Passed: 28
Failed: 2
Pass Rate: 93.33%

⚠️ MINOR ISSUES DETECTED - Review failed tests
```

**Action:**  
1. Check which tests failed: `grep "FAIL" audit_report_*.md`
2. Review test details in report
3. Fix contract if needed
4. Re-run audit

---

### 🚨 Critical Issues (<90% Pass)
```
Total Tests: 30
Passed: 20
Failed: 10
Pass Rate: 66.67%

❌ CRITICAL ISSUES DETECTED - DO NOT DEPLOY
```

**Action:**  
1. **DO NOT DEPLOY** to mainnet
2. Review all failures in report
3. Check for "CRITICAL" security issues
4. Fix contract code
5. Redeploy and re-test

---

## Common Issues and Fixes

### Issue: "Transaction failed" repeatedly
```bash
# Check if node is responsive
zigchaind status --node $NODE

# If node down, wait or try alternative
export NODE="https://zigchain-testnet-rpc.polkachu.com:443"
```

### Issue: "Insufficient funds"
```bash
# Send more tokens to test wallets
zigchaind tx bank send lp wallet2 10000000uzig \
  --node $NODE --chain-id $CHAIN_ID --gas auto --gas-adjustment 1.5 -y
```

### Issue: "Wallet not found"
```bash
# Import wallet (you'll need the mnemonic)
zigchaind keys add wallet_name --recover
```

### Issue: Script hangs during unbonding wait
**This is normal!** The script waits for the actual 300-second time-lock to expire.

**Expected behavior:**
```
[INFO] Waiting 305 seconds for unbonding to complete...
```

Just let it run. Go get coffee ☕

---

## After the Audit

### If All Tests Passed ✅
1. **Review the report** to understand what was tested
2. **Save the report** for documentation
3. **Consider deployment** to mainnet
4. **Share results** with team/auditors

### If Some Tests Failed ⚠️
1. **Don't panic** - Review which tests failed
2. **Read the details** in the report
3. **Check TEST_REFERENCE.md** for what each test validates
4. **Fix issues** in contract code
5. **Redeploy** and re-test

### If Many Tests Failed 🚨
1. **DO NOT DEPLOY** anywhere
2. **Review IMPROVEMENTS.md** to understand the test logic
3. **Debug the contract** thoroughly
4. **Consider professional audit** before mainnet

---

## Next Steps

### 1. Save the Report
```bash
REPORT=$(ls -t audit_report_*.md | head -1)
cp $REPORT audit_results/$(date +%Y%m%d)_audit.md
```

### 2. Review Documentation
- 📘 **AUDIT_GUIDE.md** - Full documentation
- 📋 **TEST_REFERENCE.md** - Quick test reference  
- 📈 **IMPROVEMENTS.md** - What's new vs old script

### 3. Share Results
```bash
# View report in browser
REPORT=$(ls -t audit_report_*.md | head -1)
markdown "$REPORT" > /tmp/audit.html
xdg-open /tmp/audit.html
```

### 4. Run Again for Consistency
```bash
# Run 2-3 times to verify consistency
./scripts/comprehensive_audit.sh
```

**Why?** Non-deterministic failures indicate timing issues or race conditions.

---

## Help & Support

### Get Help
```bash
# View full guide
cat scripts/AUDIT_GUIDE.md | less

# View test reference
cat scripts/TEST_REFERENCE.md | less

# View improvements
cat scripts/IMPROVEMENTS.md | less
```

### Debug Mode
Add to script (line 8):
```bash
set -x  # Print every command
```

### Contact Information
- Script Version: 4.0
- Created: February 14, 2026
- Compatible with: CosmWasm 2.2.4, ZigChain Testnet

---

## Ready? Let's Go! 🚀

```bash
# Make sure you're in the right directory
cd ~/Desktop_extracted/Desktop/NewFolder/lp-transfer/liquidity-protocol

# Run the audit
./scripts/comprehensive_audit.sh

# Wait for results (10-15 minutes)
# ☕ Perfect time for a coffee break!

# View the report
cat $(ls -t audit_report_*.md | head -1)
```

**Good luck! Your vault is about to be thoroughly tested.** 🎯
