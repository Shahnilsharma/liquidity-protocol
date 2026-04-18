#!/bin/bash
# K1 Keeper: disburseYield
# Fires YieldDistributor::DisburseYield per pool every 7 days (Monday)
# Pre-check: YieldReserve.balance() >= nextCycleAmount
# If balance is short, alert admin and do not fire
# Max 2h delay tolerance

POOL_ADDRESS="$1"
NODE="https://public-zigchain-testnet-rpc.numia.xyz:443"

# Query reserve balance
RESERVE_BALANCE=$(zigchaind query bank balances "$POOL_ADDRESS" --node "$NODE" -o json | jq -r '.balances[] | select(.denom=="uzig") | .amount')

# Query next cycle obligation (replace with actual query)
CYCLE_OBLIGATION=$(zigchaind query wasm contract-state smart "$POOL_ADDRESS" '{"next_cycle_obligation":{}}' --node "$NODE" -o json | jq -r '.obligation')

if [ "$RESERVE_BALANCE" -lt "$CYCLE_OBLIGATION" ]; then
  echo "[K1] ALERT: YieldReserve balance low. Required: $CYCLE_OBLIGATION, Available: $RESERVE_BALANCE"
  exit 1
fi

# Disburse yield
zigchaind tx wasm execute "$POOL_ADDRESS" '{"disburse_yield":{}}' --from keeper --node "$NODE" --gas auto --gas-adjustment 1.4 --fees 5000uzig -y

# If missed: LPs do not receive yield on time. Admin must top up reserve and re-run.