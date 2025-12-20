# Liquidity Pool Protocol

[![Rust](https://img.shields.io/badge/rust-1.81%2B-orange.svg)](https://www.rust-lang.org/)
[![CosmWasm](https://img.shields.io/badge/cosmwasm-2.3.0-blue.svg)](https://cosmwasm.com/)
[![License](https://img.shields.io/badge/license-Apache%202.0-green.svg)](LICENSE)

Production-ready liquidity pool smart contract for ZigChain using TokenFactory for native LP tokens.

## Overview

A liquidity pool contract enabling:
- **Deposit**: Send stablecoin, receive LP tokens (1:1 ratio)
- **Withdraw**: Burn LP tokens, receive stablecoin back
- **Native Tokens**: All operations use bank module (no CW20 contracts)
- **Single Transaction**: No approval step required

## Features

**TokenFactory Integration:**
- Native LP tokens (bank module denoms, not CW20)
- IBC compatible
- Visible in all Cosmos wallets

**Security:**
- No approval vulnerabilities
- Overflow protection
- Admin access controls

**Efficiency:**
- 47% gas savings vs CW20
- Single-step operations
- Optimized WASM (255KB)

## Architecture

**Flow:**
```
User → Send Stablecoin → LP Pool Contract
                              ↓
                        Mint LP Tokens
                              ↓
                    Return to User's Wallet
```

**Components:**
- **Contract**: Manages deposits/withdrawals, mints/burns LP tokens
- **TokenFactory**: Creates and manages native LP denom
- **Bank Module**: Handles all token transfers

**Comparison:**

| Metric | TokenFactory | CW20 |
|--------|-------------|------|
| Contracts | 1 | 3 |
| Approval | None | Required |
| Deposit Gas | ~195k | ~305k |
| Withdraw Gas | ~208k | ~449k |
| Total Savings | 47% | - |

## Quick Start

### Build

```bash
# Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-unknown-unknown

# Build
cargo build --release --target wasm32-unknown-unknown

# Optimize (requires Docker)
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/target \
  cosmwasm/optimizer:0.16.1
```

### Deploy

```bash
# Set environment
export WALLET="mynewwallet"
export NODE="https://public-zigchain-testnet-rpc.numia.xyz:443"
export CHAIN_ID="zig-test-2"

# Deploy using script
chmod +x scripts/deploy_tokenfactory.sh
./scripts/deploy_tokenfactory.sh

# Or manually upload
zigchaind tx wasm store artifacts/liquidity_protocol.wasm \
  --from $WALLET --node $NODE --chain-id $CHAIN_ID \
  --gas-prices 0.025uzig --gas auto --gas-adjustment 1.5 -y

# Instantiate (requires 100 ZIG for denom creation)
CODE_ID=1611  # Your uploaded code ID
zigchaind tx wasm instantiate $CODE_ID \
  '{"stablecoin_denom":"uzig","lp_subdenom":"lptoken",
    "lp_minting_cap":"1000000000000","can_change_minting_cap":false,
    "description":"LP Token","admin":"YOUR_ADDRESS"}' \
  --from $WALLET --amount 100000000uzig --no-admin \
  --node $NODE --chain-id $CHAIN_ID \
  --gas-prices 0.025uzig --gas auto --gas-adjustment 1.5 -y
```

### Interact

```bash
# Load contract addresses
source scripts/contract_addresses_tokenfactory.txt

# Deposit stablecoin → receive LP tokens
zigchaind tx wasm execute $LP_POOL_ADDRESS \
  '{"deposit":{}}' \
  --from $WALLET --amount 100uzig \
  --node $NODE --chain-id $CHAIN_ID \
  --gas-prices 0.025uzig --gas auto --gas-adjustment 1.5 -y

# Withdraw LP tokens → receive stablecoin
zigchaind tx wasm execute $LP_POOL_ADDRESS \
  '{"withdraw":{}}' \
  --from $WALLET --amount 50${LP_FULL_DENOM} \
  --node $NODE --chain-id $CHAIN_ID \
  --gas-prices 0.025uzig --gas auto --gas-adjustment 1.5 -y

# Check balances
zigchaind query bank balances $(zigchaind keys show $WALLET -a) --node $NODE

# Query pool info
zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
  '{"pool_info":{}}' --node $NODE --output json | jq '.data'
```

## Project Structure

```
├── src/
│   ├── contract.rs      # Core logic (instantiate, execute, query)
│   ├── custom.rs        # TokenFactory protobuf encoders
│   ├── msg.rs           # Message definitions
│   ├── state.rs         # State management
│   ├── error.rs         # Error types
│   └── lib.rs           # Module exports
├── scripts/
│   ├── deploy_tokenfactory.sh       # Automated deployment
│   ├── interact_tokenfactory.sh     # Interactive CLI
│   └── contract_addresses_tokenfactory.txt  # Deployed addresses
├── docs/
│   ├── QUERIES.md                   # Query reference guide
│   ├── TOKENFACTORY_MIGRATION.md    # CW20 to TokenFactory migration
│   └── guides/
│       └── DEPLOYMENT_GUIDE.md      # Detailed deployment steps
├── artifacts/           # Compiled WASM binaries
└── Cargo.toml          # Dependencies
```

## Documentation

- **[Query Guide](docs/QUERIES.md)** - Complete query reference with examples
- **[Migration Guide](docs/TOKENFACTORY_MIGRATION.md)** - Migrating from CW20
- **[Deployment Guide](docs/guides/DEPLOYMENT_GUIDE.md)** - Step-by-step deployment

## Testing

```bash
# Unit tests
cargo test

# Integration testing on testnet
source scripts/contract_addresses_tokenfactory.txt

# Test deposit
zigchaind tx wasm execute $LP_POOL_ADDRESS '{"deposit":{}}' \
  --from $WALLET --amount 100uzig --node $NODE --chain-id $CHAIN_ID \
  --gas-prices 0.025uzig --gas auto --gas-adjustment 1.5 -y

# Verify pool state
zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
  '{"pool_info":{}}' --node $NODE --output json | jq '.data'

# Test withdrawal
zigchaind tx wasm execute $LP_POOL_ADDRESS '{"withdraw":{}}' \
  --from $WALLET --amount 50${LP_FULL_DENOM} --node $NODE --chain-id $CHAIN_ID \
  --gas-prices 0.025uzig --gas auto --gas-adjustment 1.5 -y
```

## Contract Interface

### Instantiate Message

```json
{
  "stablecoin_denom": "uzig",
  "lp_subdenom": "lptoken",
  "lp_minting_cap": "1000000000000",
  "can_change_minting_cap": false,
  "description": "LP Token",
  "uri": "",
  "uri_hash": "",
  "admin": "zig1xxx..."
}
```

**Parameters:**
- `stablecoin_denom`: Native denom for deposits (e.g., "uzig")
- `lp_subdenom`: Subdenom for LP tokens (3-44 chars, lowercase, [a-z0-9-])
- `lp_minting_cap`: Maximum LP supply (must be > 0)
- `admin`: Admin address for config updates

**Note:** Requires 100,000,000 uzig (100 ZIG) denom creation fee.

### Execute Messages

**Deposit:**
```json
{"deposit": {}}
```
Send stablecoin via `--amount 100uzig` flag.

**Withdraw:**
```json
{"withdraw": {}}
```
Send LP tokens via `--amount 50<LP_DENOM>` flag.

**Update Config (Admin Only):**
```json
{
  "update_config": {
    "stablecoin_denom": "new_denom",
    "admin": "new_admin"
  }
}
```

### Query Messages

**Config:**
```bash
zigchaind query wasm contract-state smart $CONTRACT '{"config":{}}'
```
Returns: `stablecoin_denom`, `lp_full_denom`, `admin`

**Pool Info:**
```bash
zigchaind query wasm contract-state smart $CONTRACT '{"pool_info":{}}'
```
Returns: `total_stablecoin_deposited`, `total_lp_supply`, `exchange_rate`

**User Info:**
```bash
zigchaind query wasm contract-state smart $CONTRACT \
  '{"user_info":{"address":"zig1xxx..."}}'
```
Returns: `address`, `lp_balance`, `stablecoin_value`

For detailed query examples, see [docs/QUERIES.md](docs/QUERIES.md).

## TokenFactory Specifications

**LP Denom Format:**
```
coin.{contract_address}.{subdenom}
```

**Example:**
```
coin.zig1zj22cuztwnn60n5edn7mpsgpty4q5wwk4thm0jpv62w73vrssn8snzfsn8.lptoken
```

**Subdenom Rules:**
- Length: 3-44 characters
- Format: Lowercase letters, numbers, hyphens only
- Must start with lowercase letter
- Valid: `lptoken`, `lp-token`, `lp123`
- Invalid: `LP`, `ab`, `token_with_underscore`

**Requirements:**
- Minting cap must be > 0
- Denom creation requires 100 ZIG fee
- Contract becomes denom admin

## Gas Optimization

| Operation | Gas Used | vs CW20 |
|-----------|----------|---------|
| Instantiate | ~365k | - |
| Deposit | ~195k | -36% |
| Withdraw | ~208k | -54% |
| Query | 0 | 0 |

**Total savings: 47% vs CW20-based implementation**

## Security

**Input Validation:**
- Amount validation (no zero operations)
- Denom verification
- Subdenom format checks

**Overflow Protection:**
- Checked arithmetic
- Safe math functions

**Access Control:**
- Admin-only config updates
- Contract controls LP denom minting

**Error Handling:**
- Comprehensive error types
- Clear error messages
- Graceful failures

## Deployment

**Testnet (zig-test-2):**
- Contract: `zig1zj22cuztwnn60n5edn7mpsgpty4q5wwk4thm0jpv62w73vrssn8snzfsn8`
- LP Denom: `coin.zig1zj22cuztwnn60n5edn7mpsgpty4q5wwk4thm0jpv62w73vrssn8snzfsn8.lptoken`
- Code ID: `1611`
- Stablecoin: `uzig`

## Development

**Prerequisites:**
- Rust 1.81+
- wasm32-unknown-unknown target
- Docker (for optimization)
- ZigChain CLI

**Commands:**
```bash
# Check
cargo check

# Test
cargo test

# Build
cargo build --release --target wasm32-unknown-unknown

# Optimize
docker run --rm -v "$(pwd)":/code cosmwasm/optimizer:0.16.1

# Schema
cargo schema

# Format
cargo fmt

# Lint
cargo clippy -- -D warnings
```

## License

Apache 2.0 - See [LICENSE](LICENSE)

## Resources

- [ZigChain Docs](https://docs.zigchain.com/)
- [CosmWasm Docs](https://docs.cosmwasm.com/)
- [TokenFactory Spec](https://github.com/cosmos/cosmos-sdk/tree/main/x/tokenfactory)
