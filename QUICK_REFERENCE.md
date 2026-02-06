# Quick Reference - ZigChain Liquidity Vault

## 🔒 Critical Information

**Withdrawal Delay:** REQUIRED at instantiation, **IMMUTABLE** forever  
**Range:** 120 to 2,592,000 seconds (2 minutes to 30 days)  
**No Default:** Must be explicitly set

---

## Instantiation

```json
{
  "stablecoin_denom": "uzig",
  "lp_subdenom": "vaulttoken",
  "lp_minting_cap": "1000000000000",
  "can_change_minting_cap": false,
  "withdrawal_delay_seconds": 172800,
  "description": "Vault LP Token",
  "admin": "zig1..."
}
```

**Withdrawal Delay Recommendations:**
- Testing: 120 (2 minutes) - Quick iteration
- Testnet: 3600 (1 hour)
- Fast liquidity: 86400 (1 day)
- Standard: 172800 (2 days)
- High security: 604800 (7 days)

---

## Execute Messages

### Deposit
```json
{"deposit": {}}
```
**Funds:** `--amount 1000uzig`  
**Returns:** LP tokens at 1:1 ratio

### Request Withdrawal (Step 1)
```json
{"request_withdraw": {}}
```
**Funds:** `--amount 500coin.zig1abc.lptoken`  
**Effect:** Burns LP, locks stablecoin, starts time lock  
**Returns:** `withdrawal_id` in events

### Claim Withdrawal (Step 2)
```json
{"claim_withdraw": {"withdrawal_id": 0}}
```
**Funds:** None  
**Effect:** Transfers stablecoin after time lock expires  
**Errors:** `WithdrawalLocked` if too early

### Update Config (Admin)
```json
{
  "update_config": {
    "admin": "zig1...",
    "stablecoin_denom": "uzig"
  }
}
```
**Note:** ⚠️ Cannot change `withdrawal_delay`!

---

## Query Messages

### Config
```bash
{"config": {}}
```
**Returns:**
```json
{
  "stablecoin_denom": "uzig",
  "lp_full_denom": "coin.zig1...lptoken",
  "admin": "zig1...",
  "lp_minting_cap": "1000000000000",
  "can_change_minting_cap": false,
  "withdrawal_delay": 172800
}
```

### Vault Info
```bash
{"vault_info": {}}
```
**Returns:**
```json
{
  "total_stablecoin_deposited": "52150",
  "total_lp_supply": "45000",
  "total_pending_withdrawals": "7150"
}
```

### Pending Withdrawals
```bash
{"pending_withdrawals": {"address": "zig1..."}}
```
**Returns:**
```json
{
  "withdrawals": [
    {
      "id": 0,
      "lp_amount": "500",
      "stablecoin_value": "500",
      "release_time": 1735689600,
      "claimable": false
    }
  ]
}
```

### Single Withdrawal
```bash
{"withdrawal": {"address": "zig1...", "withdrawal_id": 0}}
```

### User Info
```bash
{"user_info": {"address": "zig1..."}}
```
**Returns:**
```json
{
  "address": "zig1...",
  "lp_balance": "1000"
}
```

---

## CLI Examples

### Deposit
```bash
zigchaind tx wasm execute $CONTRACT '{"deposit":{}}' \
  --from wallet --amount 1000uzig \
  --node $NODE --chain-id $CHAIN_ID --gas auto --gas-adjustment 1.5 -y
```

### Request Withdrawal
```bash
zigchaind tx wasm execute $CONTRACT '{"request_withdraw":{}}' \
  --from wallet --amount 500$LP_DENOM \
  --node $NODE --chain-id $CHAIN_ID --gas auto --gas-adjustment 1.5 -y
```

### Check Pending
```bash
zigchaind query wasm contract-state smart $CONTRACT \
  "{\"pending_withdrawals\":{\"address\":\"$MY_ADDR\"}}" \
  --node $NODE --output json | jq
```

### Claim
```bash
zigchaind tx wasm execute $CONTRACT \
  '{"claim_withdraw":{"withdrawal_id":0}}' \
  --from wallet --node $NODE --chain-id $CHAIN_ID \
  --gas auto --gas-adjustment 1.5 -y
```

---

## JavaScript/TypeScript

```typescript
import { SigningCosmWasmClient } from "@cosmjs/cosmwasm-stargate";

// Deposit
await client.execute(
  sender,
  contract,
  { deposit: {} },
  "auto",
  undefined,
  [{ denom: "uzig", amount: "1000" }]
);

// Request Withdrawal
const result = await client.execute(
  sender,
  contract,
  { request_withdraw: {} },
  "auto",
  undefined,
  [{ denom: lpDenom, amount: "500" }]
);

// Get withdrawal_id from events
const withdrawalId = result.events
  .find(e => e.type === "wasm")
  ?.attributes.find(a => a.key === "withdrawal_id")
  ?.value;

// Check if claimable
const pending = await client.queryContractSmart(
  contract,
  { pending_withdrawals: { address: sender } }
);

if (pending.withdrawals[0].claimable) {
  // Claim
  await client.execute(
    sender,
    contract,
    { claim_withdraw: { withdrawal_id: 0 } },
    "auto"
  );
}
```

---

## Error Codes

| Error | Cause | Solution |
|-------|-------|----------|
| `Unauthorized` | Not admin | Use admin wallet |
| `InvalidDenom` | Wrong token sent | Check stablecoin_denom or lp_full_denom |
| `NoFunds` | No tokens sent | Add `--amount` flag |
| `ZeroAmount` | Amount is 0 | Send positive amount |
| `ExceedsCap` | Over minting cap | Increase cap or wait for withdrawals |
| `InvalidWithdrawalDelay` | Delay out of range | Use 3600-2592000 seconds |
| `WithdrawalNotFound` | Invalid ID | Query pending_withdrawals for valid IDs |
| `WithdrawalLocked` | Time lock active | Wait for release_time |
| `InvalidFunds` | Multiple denoms sent | Send only one denom |

---

## Accounting Invariant

Always true:
```
total_stablecoin_deposited = total_lp_supply + total_pending_withdrawals
```

---

## Security Checklist

- ✅ Withdrawal delay appropriate for use case
- ✅ Tested on testnet with REAL time delays
- ✅ Admin is multi-sig (production)
- ✅ `can_change_minting_cap: false` (production)
- ✅ Minting cap set realistically (2-10x expected TVL)
- ✅ Users informed of withdrawal delay
- ✅ Monitoring for pending withdrawals
- ✅ Documentation updated with contract address

---

## Gas Costs (Approximate)

| Operation | Gas | Cost @ 0.025 |
|-----------|-----|--------------|
| Deposit | ~265k | ~0.0066 ZIG |
| Request Withdrawal | ~290k | ~0.0073 ZIG |
| Claim Withdrawal | ~285k | ~0.0071 ZIG |
| Query | 0 | Free |

**Total Withdrawal:** ~575k gas (~0.014 ZIG) for 2-step process

---

## Testnet Config

```bash
export NODE="https://public-zigchain-testnet-rpc.numia.xyz:443"
export CHAIN_ID="zig-test-2"
export GAS="--gas auto --gas-adjustment 1.5"
export GAS_PRICES="--gas-prices 0.025uzig"
```

---

## Important Notes

⚠️ **withdrawal_delay** is IMMUTABLE - choose carefully!  
⚠️ Withdrawal requires 2 transactions (request + claim)  
⚠️ Cannot cancel pending withdrawal  
⚠️ Each user has separate withdrawal ID namespace (starts at 0)  
⚠️ Admin cannot bypass time lock  
⚠️ Contract has no pause/freeze functionality

---

## Documentation

- [README.md](README.md) - Overview and getting started
- [SECURITY_AUDIT.md](SECURITY_AUDIT.md) - Security analysis
- [DEPLOYMENT.md](DEPLOYMENT.md) - Deployment guide
- [QUERIES.md](QUERIES.md) - Query reference
- [CHANGELOG.md](CHANGELOG.md) - Version history

---

## Support

**Scripts:**
- `./scripts/deploy_tokenfactory.sh` - Automated deployment
- `./scripts/interact_tokenfactory.sh` - Interactive CLI

**Build:**
```bash
cargo test                              # Run tests
cargo build --release                   # Build for testing
docker run cosmwasm/optimizer:0.17.0    # Optimize for deployment
```

**Verify:**
```bash
# Check withdrawal delay is set correctly
zigchaind query wasm contract-state smart $CONTRACT \
  '{"config":{}}' --node $NODE | jq '.data.withdrawal_delay'

# Check your pending withdrawals
zigchaind query wasm contract-state smart $CONTRACT \
  "{\"pending_withdrawals\":{\"address\":\"$MY_ADDR\"}}" \
  --node $NODE | jq '.data'
```

---

**Remember:** Always test on testnet with REAL time delays before mainnet! 🚀
