# Token Vault Contract

A simple, gas-efficient single-asset vault contract for ZigChain using CosmWasm and TokenFactory.

## Overview

This contract implements a basic token vault where users can:
- **Deposit** stablecoin and receive LP (receipt) tokens at 1:1 ratio
- **Withdraw** LP tokens and receive back the original stablecoin at 1:1 ratio

**Note:** This is a vault for depositing/withdrawing, NOT a trading DEX or AMM. There are no variable exchange rates or token swaps.

## Features

- ✅ **1:1 Ratio** - Deposit 1000 tokens, get 1000 LP tokens back
- ✅ **TokenFactory Integration** - LP tokens are native (no CW20 approvals needed)
- ✅ **Gas Efficient** - ~27% cheaper than CW20-based alternatives
- ✅ **Simple & Secure** - Straightforward deposit/withdrawal mechanism

## Contract Architecture

### State Structure
```rust
pub struct VaultState {
    pub stablecoin_denom: String,           // Accepted stablecoin (e.g., "uzig")
    pub lp_denom: String,                   // LP token denom (TokenFactory)
    pub admin: Addr,                        // Admin address
    pub lp_minting_cap: Uint128,           // Maximum LP tokens
    pub can_change_minting_cap: bool,      // Allow cap changes
    pub total_stablecoin_deposited: Uint128,
    pub total_lp_supply: Uint128,
}
```

### Execute Messages

#### 1. Deposit
Deposit stablecoin and receive LP tokens.
```json
{"deposit": {}}
```
**Funds:** Must send stablecoin with `--amount`

#### 2. Withdraw
Return LP tokens and receive stablecoin.
```json
{"withdraw": {}}
```
**Funds:** Must send LP tokens with `--amount`

#### 3. Update Minting Cap (Admin only)
Update the LP minting cap (if allowed during instantiation).
```json
{
  "update_minting_cap": {
    "new_cap": "20000000000000"
  }
}
```

### Query Messages

#### 1. Config
Get contract configuration.
```bash
zigchaind query wasm contract-state smart <CONTRACT> '{"config":{}}'
```
**Returns:**
```json
{
  "stablecoin_denom": "uzig",
  "lp_full_denom": "coin.zig1...",
  "admin": "zig1..."
}
```

#### 2. Vault Info
Get total vault state.
```bash
zigchaind query wasm contract-state smart <CONTRACT> '{"vault_info":{}}'
```
**Returns:**
```json
{
  "total_stablecoin_deposited": "52150",
  "total_lp_supply": "52150"
}
```

#### 3. User Info
Get specific user's balance and value.
```bash
zigchaind query wasm contract-state smart <CONTRACT> \
  '{"user_info":{"address":"zig1..."}}'
```
**Returns:**
```json
{
  "address": "zig1...",
  "lp_balance": "1000",
  "stablecoin_value": "1000"
}
```

## Deployment

### Prerequisites
- Docker (for WASM optimization)
- `zigchaind` CLI installed and configured
- Wallet with sufficient funds (100+ ZIG for denom creation + gas)

### Step 1: Optimize WASM
```bash
docker run --rm -v "$(pwd)":/code cosmwasm/optimizer:0.16.1
```
This creates `artifacts/liquidity_protocol.wasm` (~232 KB)

### Step 2: Upload Contract
```bash
zigchaind tx wasm store artifacts/liquidity_protocol.wasm \
  --from your_wallet \
  --node <RPC_URL> \
  --chain-id <CHAIN_ID> \
  --gas auto --gas-adjustment 1.5 \
  -y

# Get code_id from transaction
zigchaind query tx <TX_HASH>
```

### Step 3: Instantiate
```bash
zigchaind tx wasm instantiate <CODE_ID> '{
  "stablecoin_denom": "uzig",
  "lp_subdenom": "vaulttoken",
  "lp_minting_cap": "10000000000000",
  "can_change_minting_cap": false,
  "description": "Token Vault LP Token",
  "admin": "your_address"
}' \
  --from your_wallet \
  --amount 100000000uzig \
  --label "token-vault-v1" \
  --node <RPC_URL> \
  --chain-id <CHAIN_ID> \
  --gas auto --gas-adjustment 1.5 \
  -y
```

**Note:** The 100000000uzig (100 tokens) is the TokenFactory denom creation fee, NOT a deposit.

### Step 4: Save Contract Address
```bash
# Get contract address from instantiate transaction
zigchaind query tx <TX_HASH>

# Save to vault_addresses.txt
export VAULT_ADDRESS="zig1..."
export LP_FULL_DENOM="coin.zig1...vaulttoken"
export STABLECOIN_DENOM="uzig"
```

## Usage

### Using the Deployment Script
```bash
# Edit scripts/deploy_tokenfactory.sh with your configuration
./scripts/deploy_tokenfactory.sh
```

### Using the Interaction Script
```bash
# Interactive menu for deposits, withdrawals, and queries
./scripts/interact_tokenfactory.sh
```

### Manual Commands

#### Deposit
```bash
zigchaind tx wasm execute <CONTRACT> '{"deposit":{}}' \
  --from your_wallet \
  --amount 1000uzig \
  --gas auto --gas-adjustment 1.5 \
  -y
```

#### Withdraw
```bash
zigchaind tx wasm execute <CONTRACT> '{"withdraw":{}}' \
  --from your_wallet \
  --amount 500<LP_DENOM> \
  --gas auto --gas-adjustment 1.5 \
  -y
```

#### Query Vault State
```bash
zigchaind query wasm contract-state smart <CONTRACT> '{"vault_info":{}}'
```

#### Check Your LP Balance
```bash
zigchaind query bank balances your_address
```

## Gas Costs

Approximate gas costs (at 0.025 gas price):

| Operation | Gas Used | Cost |
|-----------|----------|------|
| Deposit | ~265,000 | ~6,625 tokens (~0.0066) |
| Withdraw | ~285,000 | ~7,125 tokens (~0.0071) |
| Query | 0 | Free |

**Comparison:** 27% more efficient than CW20-based alternatives.

## Security Considerations

### Built-in Protections
- ✅ Strict denom validation (only configured stablecoin accepted)
- ✅ Zero amount rejection (no dust attacks)
- ✅ Balance verification (can't withdraw more than owned)
- ✅ Overflow protection (Rust's checked arithmetic)
- ✅ Admin controls (minting cap enforcement)

### Best Practices
- Use multi-sig wallet for admin address
- Set `can_change_minting_cap: false` for production
- Set realistic minting cap based on expected TVL
- Consider professional audit for mainnet deployments with significant value

## Development

### Build
```bash
# Debug
cargo build

# Release
cargo build --release

# Optimized WASM
docker run --rm -v "$(pwd)":/code cosmwasm/optimizer:0.16.1
```

### Test
```bash
cargo test
```

### Code Structure
```
src/
├── contract.rs    # Main contract logic (deposit, withdraw, queries)
├── msg.rs        # Message definitions (ExecuteMsg, QueryMsg)
├── state.rs      # State structures (VaultState)
├── error.rs      # Custom error types
└── lib.rs        # Library exports
```

## Integration Examples

### JavaScript/TypeScript
```javascript
import { SigningCosmWasmClient } from "@cosmjs/cosmwasm-stargate";

// Deposit
const depositMsg = { deposit: {} };
const funds = [{ denom: "uzig", amount: "1000" }];
await client.execute(
  senderAddress,
  contractAddress,
  depositMsg,
  "auto",
  undefined,
  funds
);

// Withdraw
const withdrawMsg = { withdraw: {} };
const lpFunds = [{ denom: lpDenom, amount: "500" }];
await client.execute(
  senderAddress,
  contractAddress,
  withdrawMsg,
  "auto",
  undefined,
  lpFunds
);

// Query
const vaultInfo = await client.queryContractSmart(
  contractAddress,
  { vault_info: {} }
);
console.log(vaultInfo);
// { total_stablecoin_deposited: "52150", total_lp_supply: "52150" }
```

## Testnet Deployment (Reference)

**Network:** zig-test-2  
**Contract:** `zig1jj787thux4fycfh9882lelpzqmxfht5jq62r54x5x59lpr4wuvtqgmdcvm`  
**Code ID:** 1617  
**LP Denom:** `coin.zig1jj787thux4fycfh9882lelpzqmxfht5jq62r54x5x59lpr4wuvtqgmdcvm.vaulttoken`

This is a reference deployment for testing. Deploy your own for production.

## Troubleshooting

### "No funds sent" error
**Cause:** Trying to deposit without `--amount` flag  
**Solution:** Include `--amount <value>uzig` when depositing

### "Insufficient balance" error
**Cause:** Trying to withdraw more LP tokens than you own  
**Solution:** Query your LP balance first:
```bash
zigchaind query bank balances <your_address>
```

### "Invalid denom" error
**Cause:** Trying to deposit wrong token or using wrong LP denom  
**Solution:** Use exact denom from contract config query

### Transaction pending forever
**Cause:** Network congestion or incorrect RPC  
**Solution:** Wait 6-10 seconds, then query transaction hash. Try different RPC if needed.

## Comparison: CW20 vs TokenFactory

| Feature | TokenFactory (This) | CW20 Alternative |
|---------|-------------------|------------------|
| Contracts needed | 1 | 2 (token + vault) |
| Approvals required | No | Yes |
| Gas per deposit | ~265k | ~365k |
| Native balance queries | Yes | No |
| Wallet support | All wallets | CW20-aware only |
| **Recommendation** | ✅ Production | Legacy support |

## License

[Your license here - e.g., MIT, Apache 2.0]

## Version

**v1.0** - Production Ready  
**Last Updated:** December 23, 2025

---

*For questions or issues, refer to the source code documentation in the `src/` directory or consult the interaction script examples in `scripts/`.*
