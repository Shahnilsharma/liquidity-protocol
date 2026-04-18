#!/bin/bash
# K3 Keeper: Facility tenure expiry
# Calls PSPPool::ExpireFacility at tenure day
# Must fire within 1 block of expiry

POOL_ADDRESS="$1"
NODE="https://public-zigchain-testnet-rpc.numia.xyz:443"

zigchaind tx wasm execute "$POOL_ADDRESS" '{"expire_facility":{}}' --from keeper --node "$NODE" --gas auto --gas-adjustment 1.4 --fees 5000uzig -y

# If missed: PSP can draw past tenure. Keeper must be fixed and call as soon as possible.