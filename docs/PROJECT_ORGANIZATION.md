# Project Organization

This document describes the professional folder structure and organization of the Liquidity Protocol project.

## Directory Structure

```
liquidity-protocol/
├── src/                          # Smart contract source code
│   ├── contract.rs              # Core business logic (368 lines)
│   ├── error.rs                 # Custom error definitions (39 lines)
│   ├── lib.rs                   # Module exports (6 lines)
│   ├── msg.rs                   # Message type definitions (95 lines)
│   └── state.rs                 # State management (31 lines)
│
├── scripts/                      # Deployment and interaction scripts
│   ├── deploy.sh                # Automated ZigChain deployment
│   ├── interact.sh              # Testing and interaction script
│   ├── QUICK_REFERENCE.sh       # Command reference guide
│   ├── contract_addresses.txt   # Deployed contract addresses (generated)
│   └── cw20_base.wasm          # CW20 token binary (downloaded)
│
├── docs/                         # Documentation
│   ├── guides/                  # Step-by-step guides
│   │   ├── DEPLOYMENT_GUIDE.md # Comprehensive deployment instructions
│   │   ├── queries.md          # Manual testing guide with all queries
│   │   └── PHASE5_ERROR_TESTING_GUIDE.md  # Error case testing
│   ├── PROJECT_STATUS.md        # Current project status and options
│   └── TEST_RESULTS.md          # Verification results from testing
│
├── artifacts/                    # Optimized WASM binaries
│   ├── liquidity_protocol.wasm  # Optimized contract binary (242KB)
│   └── checksums.txt            # SHA256 checksums
│
├── examples/                     # Schema and example code
│   └── schema.rs                # Schema generation
│
├── tests/                        # Integration tests (empty, ready for use)
│
├── Cargo.toml                    # Rust dependencies and metadata
├── Cargo.lock                    # Locked dependency versions
├── Makefile                      # Build automation
├── README.md                     # Project overview and documentation
├── CHANGELOG.md                  # Version history and changes
├── LICENSE                       # Apache 2.0 license
├── NOTICE                        # Attribution notices
└── .gitignore                   # Git ignore rules
```

## Design Principles

### 1. Separation of Concerns
- **Source Code** (`src/`): Core contract logic only
- **Scripts** (`scripts/`): Deployment and operational tooling
- **Documentation** (`docs/`): All guides and references
- **Artifacts** (`artifacts/`): Build outputs separate from source

### 2. Clean Code Standards
- No decorative comments or emojis in source code
- Minimal, purposeful comments explaining complex logic
- Professional naming conventions throughout
- Consistent formatting and style

### 3. Professional Structure
- Industry-standard CosmWasm project layout
- Clear file naming with descriptive names
- Logical grouping of related files
- Easy navigation and maintenance

## File Descriptions

### Source Files (`src/`)

**contract.rs** (368 lines)
- Entry points: `instantiate`, `execute`, `query`, `migrate`
- Business logic: deposit and withdrawal operations
- Helper functions for CW20 interactions
- Unit tests

**error.rs** (39 lines)
- Custom error types using `thiserror`
- Descriptive error messages for debugging
- All possible contract error cases

**lib.rs** (6 lines)
- Module exports
- Public API surface definition

**msg.rs** (95 lines)
- Message type definitions
- Query response structures
- Schema annotations

**state.rs** (31 lines)
- State structure definitions
- Storage key declarations
- Contract version constants

### Scripts (`scripts/`)

**deploy.sh**
- Automated deployment to ZigChain testnet
- Handles CW20 token deployment
- LP pool instantiation
- Address persistence

**interact.sh**
- Interactive testing script
- Balance checking utilities
- Deposit and withdrawal flows
- Query helpers

**QUICK_REFERENCE.sh**
- Quick command reference
- Build instructions
- Deployment alternatives
- Testing commands

### Documentation (`docs/`)

**guides/DEPLOYMENT_GUIDE.md**
- Prerequisites and requirements
- Step-by-step deployment process
- Troubleshooting guide
- Alternative testnet options

**guides/queries.md**
- Complete manual testing guide
- All query examples with expected outputs
- Phase-by-phase testing workflow
- Quick test scripts

**guides/PHASE5_ERROR_TESTING_GUIDE.md**
- Error case testing explanations
- Why tests might fail
- How to set up error conditions
- Testing scripts for negative cases

**PROJECT_STATUS.md**
- Current deployment status
- Testnet information
- Next steps and recommendations

**TEST_RESULTS.md**
- Verification results
- Transaction hashes
- Gas usage metrics
- Production readiness assessment

## File Relationships

```
README.md
    ├── References: docs/guides/DEPLOYMENT_GUIDE.md
    ├── References: docs/guides/queries.md
    └── References: scripts/deploy.sh

scripts/deploy.sh
    ├── Generates: scripts/contract_addresses.txt
    ├── Downloads: scripts/cw20_base.wasm
    └── Uses: artifacts/liquidity_protocol.wasm

scripts/interact.sh
    ├── Loads: scripts/contract_addresses.txt
    └── References: docs/guides/queries.md

docs/guides/queries.md
    ├── Uses: scripts/contract_addresses.txt
    └── References: scripts/deploy.sh

src/contract.rs
    ├── Imports: src/error.rs
    ├── Imports: src/msg.rs
    └── Imports: src/state.rs
```

## Maintenance Guidelines

### Adding New Features
1. Update source files in `src/`
2. Add tests in `src/contract.rs` or `tests/`
3. Update documentation in `docs/`
4. Update CHANGELOG.md with changes

### Deployment Updates
1. Modify `scripts/deploy.sh` as needed
2. Update deployment guide in `docs/guides/`
3. Test on testnet before mainnet
4. Document changes in CHANGELOG.md

### Documentation Updates
1. Keep README.md as the main entry point
2. Detailed guides go in `docs/guides/`
3. Status updates in `docs/PROJECT_STATUS.md`
4. Test results in `docs/TEST_RESULTS.md`

## Git Workflow

### Ignored Files
- Build artifacts in `target/`
- Generated contract addresses
- Downloaded binaries
- IDE configuration
- Temporary test files

### Tracked Files
- All source code
- Documentation
- Scripts
- Configuration files
- License and notices

## Quick Commands

```bash
# Build
cargo wasm

# Test
cargo test

# Optimize
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/optimizer:0.16.1

# Deploy
./scripts/deploy.sh

# Interact
./scripts/interact.sh

# Test manually
# See: docs/guides/queries.md
```

## Benefits of This Structure

1. **Easy Navigation**: Logical folder structure makes finding files intuitive
2. **Professional Appearance**: Clean, organized, ready for public repositories
3. **Maintainability**: Clear separation makes updates easier
4. **Collaboration**: New developers can quickly understand project layout
5. **Scalability**: Easy to add new features without clutter
6. **Best Practices**: Follows industry standards for CosmWasm projects
7. **Documentation**: Everything is documented and discoverable

## Migration from Old Structure

Files moved during reorganization:
- `deploy.sh` → `scripts/deploy.sh`
- `interact.sh` → `scripts/interact.sh`
- `QUICK_REFERENCE.sh` → `scripts/QUICK_REFERENCE.sh`
- `DEPLOYMENT_GUIDE.md` → `docs/guides/DEPLOYMENT_GUIDE.md`
- `queries.md` → `docs/guides/queries.md`
- `PHASE5_ERROR_TESTING_GUIDE.md` → `docs/guides/PHASE5_ERROR_TESTING_GUIDE.md`
- `TEST_RESULTS.md` → `docs/TEST_RESULTS.md`
- `PROJECT_STATUS.md` → `docs/PROJECT_STATUS.md`

Files removed:
- `README.old.md` (outdated)
- `setup_project.sh` (template)
- `Developing.md` (template)
- `Importing.md` (template)
- `Publishing.md` (template)

All references updated to reflect new locations.
