CosmWasm Module
CosmWasm is a secure and efficient smart contract platform designed for the Cosmos ecosystem. On ZIGChain, developers can build smart contracts using Rust and deploy them to the blockchain, enabling decentralized applications and protocols.

This guide provides an introduction to CosmWasm on ZIGChain, including a Hello World tutorial and deployment instructions.

Prerequisites
Before developing CosmWasm smart contracts, make sure you have the required prerequisites for your platform:

Install system dependencies
Linux
macOS ARM
macOS AMD
Install the required build tools and libraries:

sudo apt update
sudo apt install -y build-essential pkg-config libssl-dev

Install Rust toolchain
Linux
macOS ARM
macOS AMD
Install Rust using rustup:

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

Add WASM compilation target
Linux
macOS ARM
macOS AMD
Add the wasm32-unknown-unknown target to your Rust toolchain:

rustup target add wasm32-unknown-unknown

Install Cargo utilities
Linux
macOS ARM
macOS AMD
Install cargo-generate and cosmwasm-check:

cargo install cargo-generate --features vendored-openssl
cargo install cosmwasm-check

Install Docker
Linux
macOS ARM
macOS AMD
Install Docker Engine:

sudo apt install -y docker.io
sudo systemctl start docker
sudo systemctl enable docker
sudo usermod -aG docker $USER

Note: Log out and log back in for the Docker group changes to take effect. Verify Docker is running:

docker --version
docker ps

Install zigchaind CLI
Linux
macOS ARM
macOS AMD
Install and configure the zigchaind CLI. For detailed instructions, see the Quick Start Guide.

Upload Whitelisting
Contract uploads are restricted to whitelisted addresses. Before deploying contracts, ensure your address is whitelisted. For more information, see CosmWasm Whitelisting.

Contract Setup
This section walks through creating, optimizing, and validating a CosmWasm contract using the official template.

Step 1: Generate the Contract
Ensure cargo-generate is installed (see the prerequisites), then generate the starter contract. For this guide, do not select the minimal template option when prompted:

cargo generate --git https://github.com/CosmWasm/cw-template.git --name hello-world
cd hello-world

Step 2: Configure Cargo
Create .cargo/config.toml to ensure WASM compatibility with bulk memory support:

mkdir -p .cargo
cat > .cargo/config.toml << 'EOF'
[alias]
wasm = "build --release --lib --target wasm32-unknown-unknown"
unit-test = "test --lib"
schema = "run --bin schema"
integration-test = "test --lib integration_tests"

[target.wasm32-unknown-unknown]
rustflags = [
  "-C", "link-arg=-s",
  "-C", "target-feature=+bulk-memory",
  "-C", "target-feature=+reference-types",
  "-C", "target-feature=+sign-ext",
]
EOF

Step 3: Compile the Contract
cargo build --release --target wasm32-unknown-unknown --lib

The compiled contract is located at: target/wasm32-unknown-unknown/release/hello_world.wasm

Step 4: Validate the Contract
cosmwasm-check target/wasm32-unknown-unknown/release/hello_world.wasm

Note: Validation is optional - the contract will be validated during deployment.

Step 5: Optimize the Contract
Optimize your contract using the CosmWasm workspace optimizer. This ensures proper compilation with bulk memory support and produces smaller, optimized contracts suitable for deployment:

Note: Ensure Docker Desktop (macOS/Windows) or the Docker daemon (Linux) is running before executing the optimizer container.

docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/workspace-optimizer:0.17.0

The optimized contract will be located at: artifacts/hello_world.wasm

Note: The artifacts/ directory and files inside will be owned by root (since Docker runs as root). This is normal and the files are still accessible for deployment. If needed, you can change ownership:

sudo chown -R $USER:$USER artifacts/

However, this is not required - you can deploy the contract directly even if it's owned by root.

Important: Before running the optimizer, ensure your Cargo.lock file uses lock version 3. If needed, update it by running:

cargo update

Deployment
Once your contract is compiled and optimized, you can deploy it to ZIGChain. The deployment process involves storing the contract, instantiating it, and interacting with it.

Step 1: Store Contract
Upload the optimized WASM file to the blockchain. After running the workspace optimizer (Step 5), use the optimized contract from the artifacts/ directory:

Mainnet
Testnet
Local
zigchaind tx wasm store artifacts/hello_world.wasm \
  --from $WALLET_ID \
  --gas auto --gas-adjustment 1.3 --gas-prices 0.0025uzig \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz \
  --yes

After the transaction completes, save the transaction hash into a shell variable:

TX_HASH=<paste_tx_hash_here>

Extract the code ID directly from the transaction events:

Mainnet
Testnet
Local
zigchaind q tx $TX_HASH \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz \
  -o json \
| jq -r '
    .events[]
    | select(.type == "store_code")
    | .attributes[]
    | select(.key == "code_id")
    | .value
  '

The command prints the code ID, which will be used for instantiation.

Step 2: Instantiate Contract
Create an instance of the contract with initial parameters:

Before running the instantiate commands, set the required shell variables:

CODE_ID=<code-id-from-store>
WALLET_ADDRESS=<your-wallet-address>

Mainnet
Testnet
Local
zigchaind tx wasm instantiate $CODE_ID \
  '{"count": 0}' \
  --label "Hello World Contract" \
  --from $WALLET_ID \
  --admin $WALLET_ADDRESS \
  --gas auto --gas-adjustment 1.3 --gas-prices 0.0025uzig \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz \
  --yes

After the transaction completes, save the transaction hash into a shell variable:

TX_HASH=<paste_tx_hash_here>

Retrieve the instantiated contract address (first match) with:

Mainnet
Testnet
Local
zigchaind q tx $TX_HASH \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz \
  -o json \
| jq -r '
    .events[]
    | select(.type == "instantiate")
    | .attributes[]
    | select(.key == "_contract_address")
    | .value
  ' | head -n1

The command prints the contract address. Save it in a shell variable:

CONTRACT_ADDRESS=<paste_contract_address_here>

Step 3: Execute Contract
Send execute messages to update the contract state:

Mainnet
Testnet
Local
zigchaind tx wasm execute $CONTRACT_ADDRESS \
  '{"increment": {}}' \
  --from $WALLET_ID \
  --gas auto --gas-adjustment 1.3 --gas-prices 0.0025uzig \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz \
  --yes

After the transaction completes, you'll receive a response similar to:

gas estimate: 134527
code: 0
codespace: ""
data: ""
events: []
gas_used: "0"
gas_wanted: "0"
height: "0"
info: ""
logs: []
raw_log: ""
timestamp: ""
tx: null
txhash: 102BDB1A57784DD823E8E668B69A095D3AE86596727F2EB62CCE386F5A611AC9

Step 4: Query Contract
Query the contract state without modifying it:

Mainnet
Testnet
Local
zigchaind query wasm contract-state smart $CONTRACT_ADDRESS \
  '{"get_count":{}}' \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz

The query will return a response similar to:

data:
  count: 1

The query returns the current count value stored in the contract. In this example, the count is 1 (after incrementing it in Step 3).

Troubleshooting
Error: "bulk memory support is not enabled"
If you encounter the following error when storing a contract:

rpc error: code = Unknown desc = failed to execute message;
message index: 0: Error calling the VM:
Error during static Wasm validation:
Wasm bytecode could not be deserialized.
Deserialization error:
"bulk memory support is not enabled (at offset 0x935)":
create wasm contract failed

This indicates that your contract was compiled without bulk memory support, but the runtime requires it. The recommended solution is to use the CosmWasm workspace optimizer, which handles bulk memory support correctly.

To resolve this:

Update your .cargo/config.toml to enable bulk memory features (as shown in Step 2 above). Ensure the rustflags section includes:

"-C", "target-feature=+bulk-memory"
"-C", "target-feature=+reference-types"
"-C", "target-feature=+sign-ext"
Use the workspace optimizer (see Step 5: Optimize the Contract above). This ensures proper compilation with bulk memory support:

docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/workspace-optimizer:0.17.0

Deploy the optimized contract:

Mainnet
Testnet
Local
zigchaind tx wasm store artifacts/hello_world.wasm \
  --from $WALLET_ID \
  --gas auto --gas-adjustment 1.3 --gas-prices 0.0025uzig \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz \
  --yes

Error: "out of gas in location: WriteFlat"
When executing contracts that perform extensive state writes, you may encounter an error like:

ERR panic recovered in runTx err="out of gas in location: WriteFlat; gasWanted: 300000, gasUsed: 300541: out of gas"


What is WriteFlat?

WriteFlat is a gas-metered KV-store write path. This error occurs when a contract execution writes enough data to state that it runs slightly over the gas limit. Contracts that perform multiple storage operations (such as updating many state entries, batch operations, or complex state transitions) are more likely to encounter this error.

Why does this happen?

Gas estimation may not always accurately predict the exact gas consumption for contracts with extensive state writes. The actual gas used can exceed the estimated amount, especially when:

The contract performs batch operations on multiple state entries
The contract updates many storage keys in a single transaction
The contract's execution path involves conditional logic that leads to more writes than estimated
Network conditions or state size affect the actual gas consumption
How to prevent WriteFlat errors:

Use a higher gas adjustment rate: Increase your --gas-adjustment parameter for contract executions. While 1.3 is standard, consider using 1.5 to 2.5 for contracts known to perform extensive writes:

Mainnet
Testnet
Local
zigchaind tx wasm execute $CONTRACT_ADDRESS \
  '{"your_message": {}}' \
  --from $WALLET_ID \
  --gas auto --gas-adjustment 1.5 --gas-prices 0.0025uzig \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz \
  --yes

Manually set a higher gas limit: For contracts with known write-heavy operations, set a higher gas limit explicitly:

Mainnet
Testnet
Local
zigchaind tx wasm execute $CONTRACT_ADDRESS \
  '{"your_message": {}}' \
  --from $WALLET_ID \
  --gas 500000 --gas-prices 0.0025uzig \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz \
  --yes

Simulate transactions first: Before broadcasting, simulate your contract execution to get a more accurate gas estimate:

Mainnet
Testnet
Local
zigchaind tx wasm execute $CONTRACT_ADDRESS \
  '{"your_message": {}}' \
  --from $WALLET_ID \
  --gas auto --gas-adjustment 1.3 \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz \
  --dry-run

Monitor and establish baselines: If you consistently encounter WriteFlat errors for specific contract operations, establish a baseline gas limit for those operations and add a safety buffer (typically 20-50% above the estimated amount).

Optimize contract design: Consider refactoring contracts to reduce the number of state writes per transaction, or split complex operations into multiple transactions when possible.

Ledger Device Issues
When deploying CosmWasm contracts using a Ledger hardware wallet, you may encounter specific issues related to transaction signing and message size limitations.

Error: APDU Error Code 0x6988
When attempting to store a contract using a Ledger device, you may encounter the following error:

Default sign-mode 'direct' not supported by Ledger, using sign-mode 'amino-json'.
gas estimate: 2681542
APDU Error Code from Ledger Device: 0x6988

Why Store Operations Fail with Ledger
The store operation fails because it includes the entire WASM binary in the transaction message. The WASM binary size typically exceeds what the Ledger Cosmos app can handle, resulting in the 0x6988 error code, which indicates insufficient space or data too large.

Why Instantiate and Execute Work
Operations like instantiate and execute work successfully with Ledger devices because they involve smaller JSON payloads that the Ledger Cosmos app can sign. These messages contain only the contract parameters, not the entire WASM binary.

Solution: Use Sign Mode amino-json
For transactions that work with Ledger (such as instantiate and execute), you must use the --sign-mode amino-json flag:

Successful Instantiation with Ledger:

Mainnet
Testnet
Local
zigchaind tx wasm instantiate $CODE_ID \
  '{"count": 0}' \
  --label "Hello World Contract" \
  --from my-ledger \
  --sign-mode amino-json \
  --admin zig1ugknvmx95z8wn7ug3w60quway69uczaku3lc6t \
  --gas auto --gas-adjustment 1.3 --gas-prices 0.0025uzig \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz \
  --yes

Example of Successful Instantiation with Ledger:

Mainnet
Testnet
Local
zigchaind tx wasm execute $CONTRACT_ADDRESS \
  '{"update_greeting":{"greeting":"Hello, World!"}}' \
  --from my-ledger \
  --sign-mode amino-json \
  --gas auto --gas-adjustment 1.3 --gas-prices 0.0025uzig \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz \
  --yes

Important Note
Store operations cannot be performed with Ledger devices due to the size limitation. To store contracts, you must use:

A software wallet (stored keys in zigchaind)
A different signing method that supports larger transaction payloads
After storing the contract using an alternative method, you can use your Ledger device for instantiate and execute operations by including the --sign-mode amino-json flag.

