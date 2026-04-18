# DeFa PSP — Testnet Deployment Runbook
# Generated: 2026-04-18
# Chain: ZigChain Testnet
# Contracts: pool-factory, psp-pool, credit-manager, yield-distributor, yield-reserve

## Prerequisites
- ZigChain CLI (`zigchaind`) installed and configured
- jq, bash, and cargo installed
- Wallets funded and admin multisig address set

## Environment Setup
```bash
export NODE="https://public-zigchain-testnet-rpc.numia.xyz:443"
export CHAIN_ID="zig-test-2"
export ADMIN_ADDR="<2-of-3 multisig address>" # Set to your multisig
export WALLET="test-wallet" # Key name in zigchaind
```

## Phase 1 — Build & Optimize
```bash
cargo build --workspace --release --lib --target wasm32-unknown-unknown
for p in pool-factory psp-pool credit-manager yield-distributor yield-reserve; do \
  cp target/wasm32-unknown-unknown/release/defa_${p}.wasm artifacts/defa_${p}.wasm; \
  echo "Prepared: artifacts/defa_${p}.wasm"; \
done
```

## Phase 2 — Deploy PoolFactory (one-time singleton)
### Step 2.1 — Upload WASMs
**Trigger:** New deployment or upgrade
**Actor:** Admin

```bash
bash scripts/deploy_v2_stack.sh
```

**Verify:**
- Check scripts/v2_addresses.txt for all code IDs and PoolFactory address

**Expected output:**
- All code IDs and PoolFactory address exported

**If output does not match:** Check upload transactions and wallet balance

## Phase 3 — Create Pool via PoolFactory
### Step 3.1 — Create Pool
**Trigger:** After PoolFactory deployed
**Actor:** Admin

```bash
# Edit scripts/deploy_defa_v2.sh with your pool parameters, then run:
bash scripts/deploy_defa_v2.sh --wallet $WALLET
```

**Verify:**
- scripts/vault_addresses.txt contains all per-pool contract addresses

**Expected output:**
- All contract addresses exported

**If output does not match:** Check PoolFactory logs and init payloads

## Phase 4 — Verify Deployment
### Step 4.1 — Query contract state
**Trigger:** After deployment
**Actor:** Admin

```bash
zigchaind query wasm contract-state smart $POOL_FACTORY_ADDRESS '{"config":{}}' --node $NODE -o json
```

**Expected output:**
- JSON with correct admin and code IDs

**If output does not match:** Check instantiation parameters

## Phase 5 — Fund YieldReserve
### Step 5.1 — Fund contract
**Trigger:** Before opening fundraising
**Actor:** Admin

```bash
zigchaind tx bank send $ADMIN_ADDR $YIELD_RESERVE_ADDRESS 100000000uzig --from $WALLET --node $NODE --chain-id $CHAIN_ID --gas auto --gas-adjustment 1.4 --fees 5000uzig -y
```

**Verify:**
- Query balance:
```bash
zigchaind query bank balances $YIELD_RESERVE_ADDRESS --node $NODE
```

**Expected output:**
- Sufficient uzig balance

**If output does not match:** Check funding transaction

## Phase 6 — Open Fundraising (LP Deposits)
### Step 6.1 — Open fundraising
**Trigger:** After funding YieldReserve
**Actor:** Admin

```bash
zigchaind tx wasm execute $PSP_POOL_ADDRESS '{"open_fundraising":{}}' --from $WALLET --node $NODE --chain-id $CHAIN_ID --gas auto --gas-adjustment 1.4 --fees 5000uzig -y
```

**Verify:**
- Query state:
```bash
zigchaind query wasm contract-state smart $PSP_POOL_ADDRESS '{"state":{}}' --node $NODE -o json
```

**Expected output:**
- "state": "Fundraising"

**If output does not match:** Check contract logs

## Phase 7 — Execute Facility (transition to Active)
### Step 7.1 — Execute facility
**Trigger:** Fundraising complete
**Actor:** Admin

```bash
zigchaind tx wasm execute $PSP_POOL_ADDRESS '{"execute_facility":{}}' --from $WALLET --node $NODE --chain-id $CHAIN_ID --gas auto --gas-adjustment 1.4 --fees 5000uzig -y
```

**Verify:**
- Query state:
```bash
zigchaind query wasm contract-state smart $PSP_POOL_ADDRESS '{"state":{}}' --node $NODE -o json
```

**Expected output:**
- "state": "Active"

**If output does not match:** Check contract logs

## Phase 8 — PSP Drawdown
### Step 8.1 — Request drawdown
**Trigger:** Facility is Active
**Actor:** PSP

```bash
zigchaind tx wasm execute $CREDIT_MANAGER_ADDRESS '{"request_drawdown":{"amount":"1000000"}}' --from $PSP_KEY --node $NODE --chain-id $CHAIN_ID --gas auto --gas-adjustment 1.4 --fees 5000uzig -y
```

**Verify:**
- Query outstanding:
```bash
zigchaind query wasm contract-state smart $CREDIT_MANAGER_ADDRESS '{"outstanding":{}}' --node $NODE -o json
```

**Expected output:**
- "outstanding": "1000000"

**If output does not match:** Check drawdown limits and state

## Phase 9 — PSP Repay
### Step 9.1 — Repay
**Trigger:** After drawdown
**Actor:** PSP

```bash
zigchaind tx wasm execute $CREDIT_MANAGER_ADDRESS '{"repay":{"amount":"1000000"}}' --from $PSP_KEY --node $NODE --chain-id $CHAIN_ID --gas auto --gas-adjustment 1.4 --fees 5000uzig -y
```

**Verify:**
- Query outstanding:
```bash
zigchaind query wasm contract-state smart $CREDIT_MANAGER_ADDRESS '{"outstanding":{}}' --node $NODE -o json
```

**Expected output:**
- "outstanding": "0"

**If output does not match:** Check repayment logic

## Phase 10 — LP Withdrawal Request (Monday only)
### Step 10.1 — Request withdrawal
**Trigger:** Monday, after facility Active
**Actor:** LP

```bash
zigchaind tx wasm execute $PSP_POOL_ADDRESS '{"request_withdrawal":{}}' --from $LP_KEY --node $NODE --chain-id $CHAIN_ID --gas auto --gas-adjustment 1.4 --fees 5000uzig -y
```

**Verify:**
- Query withdrawal queue:
```bash
zigchaind query wasm contract-state smart $PSP_POOL_ADDRESS '{"withdrawal_queue":{}}' --node $NODE -o json
```

**Expected output:**
- LP address in queue

**If output does not match:** Check day-of-week logic

## Phase 11 — LP Claim Withdrawal
### Step 11.1 — Claim withdrawal
**Trigger:** After withdrawal request processed
**Actor:** LP

```bash
zigchaind tx wasm execute $PSP_POOL_ADDRESS '{"claim_withdrawal":{}}' --from $LP_KEY --node $NODE --chain-id $CHAIN_ID --gas auto --gas-adjustment 1.4 --fees 5000uzig -y
```

**Verify:**
- Query LP balance:
```bash
zigchaind query bank balances $LP_ADDR --node $NODE
```

**Expected output:**
- Increased uzig balance

**If output does not match:** Check claim logic

## Phase 12 — Yield Disbursal (K1 Keeper)
### Step 12.1 — Disburse yield
**Trigger:** Every 7 days (Monday)
**Actor:** Keeper

```bash
bash scripts/keepers/k1_disburse_yield.sh $YIELD_DISTRIBUTOR_ADDRESS
```

**Verify:**
- Query yield disbursed:
```bash
zigchaind query wasm contract-state smart $YIELD_DISTRIBUTOR_ADDRESS '{"last_disbursed":{}}' --node $NODE -o json
```

**Expected output:**
- Updated timestamp

**If output does not match:** Check reserve balance and keeper logs

## Phase 13 — Expire Facility (K3 Keeper)
### Step 13.1 — Expire facility
**Trigger:** At tenure expiry
**Actor:** Keeper

```bash
bash scripts/keepers/k3_expire_facility.sh $PSP_POOL_ADDRESS
```

**Verify:**
- Query state:
```bash
zigchaind query wasm contract-state smart $PSP_POOL_ADDRESS '{"state":{}}' --node $NODE -o json
```

**Expected output:**
- "state": "WindingDown"

**If output does not match:** Check expiry logic

## Phase 14 — Final LP Principal Withdrawal
### Step 14.1 — Final withdrawal
**Trigger:** After facility settled
**Actor:** LP

```bash
zigchaind tx wasm execute $PSP_POOL_ADDRESS '{"claim_withdrawal":{}}' --from $LP_KEY --node $NODE --chain-id $CHAIN_ID --gas auto --gas-adjustment 1.4 --fees 5000uzig -y
```

**Verify:**
- Query LP balance:
```bash
zigchaind query bank balances $LP_ADDR --node $NODE
```

**Expected output:**
- Final principal returned

**If output does not match:** Check settlement logic

## Phase 15 — Admin Close Pool
### Step 15.1 — Close pool
**Trigger:** After all obligations settled
**Actor:** Admin

```bash
zigchaind tx wasm execute $PSP_POOL_ADDRESS '{"close_pool":{}}' --from $WALLET --node $NODE --chain-id $CHAIN_ID --gas auto --gas-adjustment 1.4 --fees 5000uzig -y
```

**Verify:**
- Query state:
```bash
zigchaind query wasm contract-state smart $PSP_POOL_ADDRESS '{"state":{}}' --node $NODE -o json
```

**Expected output:**
- "state": "Closed"

**If output does not match:** Check close logic

## Appendix A — Keeper Operations
- K1: scripts/keepers/k1_disburse_yield.sh
- K2: scripts/keepers/k2_overdue_monitor.sh
- K3: scripts/keepers/k3_expire_facility.sh
- K4: scripts/keepers/k4_reserve_health.sh

## Appendix B — Emergency Procedures
- If any keeper script fails, check logs and rerun after resolving the issue
- For contract pause, use:
```bash
zigchaind tx wasm execute $PSP_POOL_ADDRESS '{"pause":{}}' --from $WALLET --node $NODE --chain-id $CHAIN_ID --gas auto --gas-adjustment 1.4 --fees 5000uzig -y
```

## Appendix C — Wasm Artifact Checksums

| Contract                   | Size  | SHA256                                                         |
|----------------------------|-------|----------------------------------------------------------------|
| defa_pool_factory.wasm     | 277K  | af60ace9871f9cfdab365b1c74eec3ccbe1f171879b8106437c6bb95d7b6a1f3 |
| defa_psp_pool.wasm         | 348K  | dcb9e6662499313f558642ae07bf17db798810d6dc7c2184693e1f5f3ea9adec |
| defa_credit_manager.wasm   | 302K  | eac54ac5e5f4c2d18c79e08d9555f189ecaa5b757ee0afe6a4a770146bcf0a57 |
| defa_yield_distributor.wasm| 265K  | b39500a7814eb1ce94fd1cbc62b44d275cd1c4847bd26bf6fbb602e366101641 |
| defa_yield_reserve.wasm    | 246K  | ca4a9954fb678f2ef07a4952bf780bef7976fd556a95ea81cc1c993f335fe034 |

**All artifact sizes are within ZigChain limits and audit reference.**

---

All 24 blockers resolved. DEPLOYMENT_RUNBOOK.md generated. Ready for onchain testnet deployment.
