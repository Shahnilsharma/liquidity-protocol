# ZigChain Institutional Staking Strategy

_Last updated: 2026-03-25_

## 1. Chain & Vault Snapshot (Testnet)
- **Vault contract**: `zig1l7kpzmynguy09ln4ar8tsrna7v09uzxrk27440aw7l72reaz3hgs0mqr4j`
- **Admin wallet**: `zig1pvucnrgua60k4kdzzawcq4qnx0pgq4zu7zdzdd`
- **Current vault state** (`zigchaind query wasm ... {"vault_info":{}}`)
  - `total_deposited = 10,000,000 uzig`
  - `total_lp_supply = 5,000,000`
  - `price_per_share = 2.000000`
  - `total_pending_withdrawals = 5,000,000 uzig`
- **Admin wallet balance**: 12,001,713 uzig liquid (funds already withdrawn from vault)
- **Staking params**: `unbonding_time = 168h0m0s` (7 days), `bond_denom = uzig`, `max_validators = 12`
- **Observed validator set**: 30+ active validators, commissions 5–10%; at least 3 currently jailed

_Assumption_: The production deployment will retain the same contract semantics (ERC-4626 accounting, 120s withdrawal delay) and only the RPC / chain ID / validator list will change.

## 2. Strategy Objectives
1. **Capital preservation** — prioritize validators with clean slashing history, high uptime, reasonable commission (≤10%).
2. **Multi-operator diversification** — no validator holds more than **22%** of staked principal; target 6–8 validators minimum, selected from curated allowlist.
3. **Liquidity awareness** — maintain a configurable **liquidity buffer** (default 12%) plus all uncovered pending withdrawals inside the vault bank balance. Always cover `pending_withdrawals` with `vault_bank + in_flight_unbondings`.
4. **Auto-compounding** — harvest rewards every 6 hours (configurable) and recycle via `admin_deposit_yield(principal=0, yield=net_rewards)`.
5. **Operational resilience** — daemon fails safe, retries idempotently, and provides reconciliation logic for orphaned funds.

## 3. Validator Selection & Governance
- **Data sources**: `zigchaind query staking validators`, `slashing signing-info`, external uptime dashboards when available.
- **Allowlist construction**:
  1. Filter out jailed validators and those with commission > 10%.
  2. Prefer foundation / well-known operators (e.g., `ZIG Sentinel`, `Stake&Relax`, `Cosmostation`, `Numia`, `Simply Staking`).
  3. Ensure geographic / operator diversity (no more than two validators from the same organization).
- **On-chain checks before every delegation**:
  - `validator.jailed == false`
  - `validator.status == BOND_STATUS_BONDED`
  - `validator.commission <= MAX_COMMISSION` (config default: 0.10)

## 4. Allocation Rules
- **Target stake per validator**: `target_weight = min(global_cap, max( floor(total_staked / target_validator_count), MIN_CHUNK ))`
- **Global cap**: `MAX_VALIDATOR_SHARE = 22%` of `total_staked_principal`.
- **Minimum delegation chunk**: 250,000 uzig (avoids dust TXs).
- **Initial distribution example** (for 10M staked):
  | Validator (moniker) | Operator | Weight | Cap |
  | --- | --- | --- | --- |
  | ZIG Sentinel | `zigvaloper1pww…` | 18% | ≤22% |
  | Numia | `zigvaloper16qp…` | 16% | ≤22% |
  | Stake&Relax | `zigvaloper1z47…` | 16% | ≤22% |
  | Cosmostation | `zigvaloper1l4x…` | 16% | ≤22% |
  | Simply Staking | `zigvaloper1gdds…` | 16% | ≤22% |
  | Meria | `zigvaloper180ke…` | 9% | ≤22% |
  | BTCS | `zigvaloper1e6v7…` | 9% | ≤22% |
- **Dynamic adjustments**: When `total_staked_principal` changes, recompute desired stake per validator and queue follow-up delegate / undelegate ops if deviation exceeds 3% absolute share or 250k uzig.

## 5. Liquidity Buffer & Unstaking Logic
- **Inputs**:
  - `vault_bank` — live contract balance (liquid)
  - `total_pending` — pending withdrawals from contract state
  - `total_unbonding` — sum of principal in-flight (local state + `query staking unbonding-delegation`)
  - `buffer_pct` — default 12% (configurable per deployment)
- **Derived values**:
  - `buffer_amount = ceil(buffer_pct * total_deposited)`
  - `pending_uncovered = max(total_pending - total_unbonding, 0)`
  - `safe_to_stake = vault_bank - buffer_amount - pending_uncovered`
- **Actions**:
  - If `safe_to_stake >= MIN_STAKE_CHUNK`, delegate according to allocation rules.
  - If `vault_bank + total_unbonding < total_pending + buffer_amount`, initiate emergency unbond equal to the shortfall (split across validators with largest overweight).
  - Always log the liquidity equation to detect drift: `vault_bank + delegated + total_unbonding ?= total_deposited`.

## 6. Reward Harvesting & Compounding
- **Cadence**: every 6 hours (config `REWARD_COLLECT_INTERVAL`), or when accrued rewards exceed 50,000 uzig.
- **Process**:
  1. Query per-validator distribution rewards or aggregate via `distribution rewards ADMIN VALIDATOR`.
  2. Withdraw rewards for all validators sequentially (respect rate limits).
  3. Sum net rewards, subtract `GAS_BUFFER` (default 20,000 uzig) to keep wallet solvent.
  4. Call `admin_deposit_yield(principal=0, yield=net_rewards_after_buffer)`.
- **Fail-safe**: if rewards query fails, back off and try next interval; never attempt deposit with zero or negative yield.

## 7. Dynamic Rebalancing & Risk Response
- **Risk triggers**:
  - Validator jailed or slashed → immediately mark for exit; queue `undelegate` of full position.
  - `actual_share > MAX_VALIDATOR_SHARE + 2%` → queue partial undelegate down to target.
  - `actual_share < target_share - 3%` and `safe_to_stake` positive → queue new delegation.
  - Performance telemetry (if available) showing uptime < 98% over last 24h → reduce weight by 50% until metrics recover.
- **Execution throttling**:
  - Max 2 staking txs per cycle (1 delegate + 1 undelegate) to avoid spamming.
  - Batch redelegations using `MsgBeginRedelegate` when moving stake between validators without waiting full unbonding period.
- **State tracking**:
  - Maintain local JSON of validator targets, actuals, and statuses (`validators.json`).
  - Persist outstanding operations with timestamps to reconcile after daemon restarts.

## 8. Agent / Daemon Loop (every 30s)
1. **Fetch chain state**
   - `vault_info`, `bank balance`, `pending withdrawals`
   - Per-validator delegations, unbondings, rewards
   - Slashing / jailing info for allowlisted validators
2. **Handle completed unbondings**
   - If `initiated_epoch + unbonding_time <= now` and admin wallet received funds → call `admin_deposit_yield(principal=amount, yield=0)`
3. **Recompute liquidity + coverage**
   - Evaluate `safe_to_stake` and `coverage_shortfall`
   - Initiate stake/unbond actions accordingly
4. **Allocation enforcement**
   - Compare `actual_share` vs `target_share` for each validator
   - Queue delegates / redelegates / undelegates while respecting caps and throttles
5. **Reward harvest (if interval elapsed)**
   - Collect rewards, deposit as yield
6. **Risk monitoring**
   - Detect jailed/slashed validators; mark for exit; alert
7. **Reconciliation**
   - Verify accounting identity and orphaned admin funds. If `admin_balance` >> `expected_buffer`, delegate or return to vault.
8. **Persist state + sleep**
   - Write JSON snapshot of vault totals, validator positions, pending ops, timestamps.

_Pseudocode summary_:
```
while true:
  state <- query_all()
  process_completed_unbondings(state)
  safe_to_stake <- compute_liquidity(state)
  if safe_to_stake >= MIN_DELEGATE: delegate_chunks(safe_to_stake)
  coverage_gap <- compute_coverage_gap(state)
  if coverage_gap > 0: queue_emergency_unbond(coverage_gap)
  enforce_validator_caps(state)
  if rewards_due(state): harvest_and_compound()
  monitor_risk(state)
  reconcile_admin_balance(state)
  persist_state(state)
  sleep(POLL_INTERVAL)
```

## 9. Edge Case Playbook
| Scenario | Detection | Action |
| --- | --- | --- |
| **Spike in withdrawals** | `total_pending` jump, `vault_bank + unbonding < pending + buffer` | Immediately unbond shortfall from overweight validators; pause new delegations until coverage restored. |
| **Validator jailed** | `validator.status != BONDED` | Trigger `MsgBeginRedelegate` to a standby validator if possible; otherwise undelegate. Alert operators. |
| **Slashing event** | Monitor `/slashing signing-info` | Halt new delegations to validator, schedule exit after unbonding; record loss in state. |
| **RPC failure** | Queries error | Retry with exponential backoff; if all nodes fail, skip staking actions that cycle, log critical alert. |
| **Partial tx failure** | `submit_tx` returns success but follow-up action missing | Compare `delegated` vs expected delta; reconciliation step redelegates or re-queues. |
| **State desync after restart** | Missing local JSON | Reconstruct by re-querying chain (delegations, unbondings) before taking actions. |
| **Admin wallet low balance** | `admin_balance < MIN_GAS_RESERVE` | Pause staking/unbond txs, alert to refill. |

## 10. Implementation Notes
- **Configuration**: externalize chain/env variables (`NODE`, `CHAIN_ID`, contract address, denom) plus strategy parameters (`buffer_pct`, `max_validator_share`, `min_delegate_amount`, `reward_interval`).
- **Data storage**: use a single JSON state file with sections `{vault, liquidity, validators, pending_ops}`; guard writes with atomic `mv`.
- **Logging**: include `[VALIDATOR:<moniker>]` tags for clarity; surface tx hashes at `INFO` level.
- **Testing**:
  - Simulate deposit/withdraw scenarios on testnet using wallets to ensure coverage calculations behave.
  - Rebalance test: artificially tweak `validators.json` and confirm daemon issues redelegations.
  - Failure injection: temporarily point to invalid RPC to verify fail-safe behavior.

## 11. Next Steps
1. **Implement validator registry** (`config/validators.json`) with weights and metadata.
2. **Extend `vault_daemon.sh`** to:
   - Track per-validator stakes (via `staking delegation` query)
   - Support redelegations (`tx staking redelegate`)
   - Apply allocation logic described above
3. **Add monitoring hooks** (Prometheus or log shipping) to alert on liquidity shortfalls or jailed validators.
4. **Prepare production deployment guide** — convert Bash daemon to a more robust agent (Rust/Python) with better observability before mainnet.

This document should be kept under version control and updated whenever parameters or validator sets change so future prompts can reference it directly.
