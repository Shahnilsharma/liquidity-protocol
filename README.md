# Liquidity Protocol - CosmWasm Smart Contract

[![Rust](https://img.shields.io/badge/rust-1.81%2B-orange.svg)](https://www.rust-lang.org/)
[![CosmWasm](https://img.shields.io/badge/cosmwasm-2.3.0-blue.svg)](https://cosmwasm.com/)
[![License](https://img.shields.io/badge/license-Apache%202.0-green.svg)](LICENSE)

A professional, production-ready liquidity pool smart contract for CosmWasm chains, implementing the standard LP token pattern with CW20 stablecoins.

## Features

- **Secure Deposits**: Users deposit CW20 stablecoins (e.g., USDT) with proper approval pattern
- **LP Token Minting**: Automatic minting of liquidity provider tokens on deposit
- **Withdraw Mechanism**: Burn LP tokens to reclaim underlying stablecoins
- **1:1 Exchange Rate**: Simple, transparent value preservation (customizable)
- **Admin Controls**: Configuration updates for authorized addresses only
- **Query Interface**: Complete transparency with pool and user queries

## Pattern Implementation

This contract follows the **standard liquidity protocol pattern**:

1. **Approval Phase**: User approves the contract to spend their stablecoins
2. **Deposit Phase**: Contract pulls stablecoins via `transfer_from`, mints LP tokens 1:1
3. **Holding Phase**: User holds LP tokens representing their stake in the pool
4. **Withdrawal Phase**: User approves LP tokens, contract burns them, returns stablecoins

## Project Structure

```
liquidity-protocol/
├── src/                    # Smart contract source code
│   ├── contract.rs        # Core business logic
│   ├── msg.rs            # Message definitions
│   ├── state.rs          # State management
│   ├── error.rs          # Custom error types
│   └── lib.rs            # Module exports
├── scripts/               # Deployment and interaction scripts
│   ├── deploy.sh         # Automated deployment
│   ├── interact.sh       # Testing interactions
│   ├── QUICK_REFERENCE.sh # Command reference
│   └── contract_addresses.txt # Deployed addresses
├── docs/                  # Documentation
│   ├── guides/           # Testing and deployment guides
│   │   ├── DEPLOYMENT_GUIDE.md
│   │   ├── queries.md
│   │   └── PHASE5_ERROR_TESTING_GUIDE.md
│   ├── PROJECT_STATUS.md
│   └── TEST_RESULTS.md
├── artifacts/            # Optimized WASM binaries
├── examples/             # Schema generation
└── Cargo.toml           # Rust dependencies
```

## Security Features

- **Amount Validation**: Prevents zero-value operations
- **Balance Verification**: Checks before transfers to prevent underflow
- **Overflow Protection**: Uses `checked_add` and `checked_sub` throughout
- **Authorization**: Admin-only functions properly gated
- **Approval Pattern**: Secure token transfers following CW20 spec
- **State Consistency**: Pool state tracking matches token supplies

## Quick Start

### Prerequisites
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-unknown-unknown

# Install Docker
sudo apt-get install docker.io
```

### Build
```bash
# Development build
cargo wasm

# Optimized production build
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/optimizer:0.16.1
```

### Test
```bash
cargo test
```

## Deployment

See [docs/guides/DEPLOYMENT_GUIDE.md](docs/guides/DEPLOYMENT_GUIDE.md) for comprehensive deployment instructions including:
- ZigChain testnet configuration
- Permission requirements
- Step-by-step deployment
- Testing procedures
- Alternative testnet options

### Quick Deploy
```bash
# Make executable
chmod +x scripts/deploy.sh

# Deploy to ZigChain testnet
./scripts/deploy.sh

# Load deployed addresses
source scripts/contract_addresses.txt
```

## Contract Interface

### Instantiate
```json
{
  "stablecoin_address": "zig1...",
  "lp_token_address": "zig1...",
  "admin": "zig1..."
}
```

### Execute Messages

#### Deposit Stablecoins
```json
{
  "deposit": {
    "amount": "1000000000"
  }
}
```
*Prerequisite*: User must approve contract via `increase_allowance` on stablecoin contract

#### Withdraw Stablecoins
```json
{
  "withdraw": {
    "amount": "500000000"
  }
}
```
*Prerequisite*: User must approve contract via `increase_allowance` on LP token contract

#### Update Config (Admin Only)
```json
{
  "update_config": {
    "stablecoin_address": "zig1...",
    "lp_token_address": "zig1...",
    "admin": "zig1..."
  }
}
```

### Query Messages

#### Get Configuration
```json
{
  "config": {}
}
```
Returns: `stablecoin_address`, `lp_token_address`, `admin`

#### Get Pool Info
```json
{
  "pool_info": {}
}
```
Returns: `total_stablecoin_deposited`, `total_lp_supply`, `exchange_rate`

#### Get User Info
```json
{
  "user_info": {
    "address": "zig1..."
  }
}
```
Returns: `address`, `lp_balance`, `stablecoin_value`

## Usage Example

### Complete Flow
```bash
# Setup
export POOL="zig1poolcontract..."
export STABLE="zig1stablecoin..."
export LP="zig1lptoken..."
export USER="zig1useraddress..."

# 1. User approves stablecoin spending
zigchaind tx wasm execute $STABLE \
  '{"increase_allowance":{"spender":"'$POOL'","amount":"1000000000"}}' \
  --from $USER --gas 300000 --fees 20000uzig -y

# 2. User deposits to pool
zigchaind tx wasm execute $POOL \
  '{"deposit":{"amount":"1000000000"}}' \
  --from $USER --gas 400000 --fees 25000uzig -y

# 3. Check LP balance
zigchaind query wasm contract-state smart $LP \
  '{"balance":{"address":"'$USER'"}}'

# 4. User approves LP token for withdrawal
zigchaind tx wasm execute $LP \
  '{"increase_allowance":{"spender":"'$POOL'","amount":"500000000"}}' \
  --from $USER --gas 300000 --fees 20000uzig -y

# 5. User withdraws from pool
zigchaind tx wasm execute $POOL \
  '{"withdraw":{"amount":"500000000"}}' \
  --from $USER --gas 400000 --fees 25000uzig -y
```

## Testing

### Unit Tests
```bash
cargo test
```

### Manual Testing
See [docs/guides/queries.md](docs/guides/queries.md) for comprehensive manual testing guide covering:
- Initial state verification
- Deposit flow testing
- Withdrawal flow testing
- Error case validation
- Complete test scripts

### Integration Tests
```bash
# Deploy contracts to testnet
./scripts/deploy.sh

# Run interaction tests
./scripts/interact.sh
```

## Development

### Code Quality Standards
- Rust 2021 Edition with modern features
- No Clippy warnings - clean, idiomatic code
- Comprehensive unit tests
- All public APIs documented
- Descriptive errors with context
- Strong typing throughout

### Dependencies (Latest Stable)
```toml
cosmwasm-std = "2.3.0"
cw-storage-plus = "2.0.0"
cw2 = "2.0.0"
cw20 = "2.0.0"
```

## Gas Optimization

- Optimized WASM binary: ~242KB
- Efficient storage patterns with `cw-storage-plus`
- Minimal storage reads/writes
- Batched operations where possible

## Migration Support

Contract includes migration entry point for future upgrades:
```rust
pub fn migrate(deps: DepsMut, env: Env, msg: MigrateMsg) -> Result<Response, ContractError>
```

## License

Apache 2.0 - see [LICENSE](LICENSE) file

## Resources

- **CosmWasm Docs**: https://docs.cosmwasm.com/
- **CW20 Specification**: https://github.com/CosmWasm/cw-plus/tree/main/packages/cw20
- **ZigChain Docs**: https://docs.zigchain.com/
- **CosmWasm Academy**: https://academy.cosmwasm.com/

## Disclaimer

This contract is provided as-is for educational and development purposes. **Not audited**. Use at your own risk. Always conduct thorough testing and security audits before deploying to production.

## Educational Value

This contract demonstrates:
- Professional CosmWasm contract structure
- CW20 token interaction patterns
- Secure approval and transfer flows
- State management best practices
- Error handling strategies
- Query implementation
- Testing methodologies

Perfect for learning CosmWasm development or as a foundation for custom liquidity protocols.
