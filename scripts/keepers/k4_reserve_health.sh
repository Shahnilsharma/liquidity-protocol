#!/bin/bash
# K4 Keeper: YieldReserve balance health check
# Runs 48h before each cycle
# Compares YieldReserve.balance() to YieldDistributor.nextCycleAmount()
# Alerts admin if deficit

POOL_ADDRESS="$1"
NODE="https://public-zigchain-testnet-rpc.numia.xyz:443"

RESERVE_BALANCE=$(zigchaind query bank balances "$POOL_ADDRESS" --node "$NODE" -o json | jq -r '.balances[] | select(.denom=="uzig") | .amount')
CYCLE_OBLIGATION=$(zigchaind query wasm contract-state smart "$POOL_ADDRESS" '{"next_cycle_obligation":{}}' --node "$NODE" -o json | jq -r '.obligation')

if [ "$RESERVE_BALANCE" -lt "$CYCLE_OBLIGATION" ]; then
  echo "[K4] ALERT: YieldReserve balance low. Required: $CYCLE_OBLIGATION, Available: $RESERVE_BALANCE"
  exit 1
fi

# If missed: Admin may not top up reserve in time, risking failed yield disbursement.