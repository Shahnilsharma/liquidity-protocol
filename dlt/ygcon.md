
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ zigchaind tx wasm store artifacts/simple_lending_borrowing.wasm   --from test-wallet   --gas auto --gas-adjustment 1.3 --gas-prices 0.0025uzig   --chain-id zig-test-2   --node https://publi
c-zigchain-testnet-rpc.numia.xyz   --yes
gas estimate: 1956667
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
txhash: FD7645AFE362B96646B2DEA841235946FF90B3D430F2FD2AA1503AB17A6C9A71
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ TX_HASH=FD7645AFE362B96646B2DEA841235946FF90B3D430F2FD2AA1503AB17A6C9A71
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ zigchaind q tx $TX_HASH \
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
1979
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ CODE_ID=1979
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ WALLET_ADDRESS=zig1ug335mpcdn2vpk8p08v4k9z7cqtdg0jj4tqr92
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ zigchaind tx wasm instantiate $C
ODE_ID \
  '{"denom": "uzig", "interest_rate": "50000"}' \
  --label "ZIG Yield Generator" \
  --from test-wallet \
  --admin test-wallet \
  --gas auto --gas-adjustment 1.3 --gas-prices 0.0025uzig \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz \
  --yes
gas estimate: 196431
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
txhash: 17A4AA338592A29806CB575D48E40AE11E91349FAC47AC714B9886DD357F1E1F
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ TX_HASH=17A4AA338592A29806CB575D48E40AE11E91349FAC47AC714B9886DD357F1E1F
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ zigchaind q tx $TX_HASH \
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
zig188jwfa9tcxed2wdav5faj6vslp0vsq5lnu9yyn0wwg75fmkxv5ussdw5nu
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ CONTRACT_ADDRESS=CONTRACT_ADDRESS=CONTRACT_ADDRESS=^C
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ CONTRACT_ADDRESS=zig188jwfa9tcxed2wdav5faj6vslp0vsq5lnu9yyn0wwg75fmkxv5ussdw5nu
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$






shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ export CONTRACT_ADDRESS=zig188jwfa9tcxed2wdav5faj6vslp0vsq5lnu9yyn0wwg75fmkxv5ussdw5nu
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$  zigchaind q bank balances test-wallet
balances:
- amount: "621709745"
  denom: uzig
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ zigchaind tx wasm execute $CONTRACT_ADDRESS   '{"deposit": {}}'   --amount 20000000uzig   --from test-wallet   --gas auto --gas-adjustment 1.3 --gas-prices 0.0025uzig   --chain-id zig-test-2   --node https://public-zigchain-testnet-rpc.numia.xyz   -y
gas estimate: 170263
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
txhash: 994A4FC337F7931E5D1D634D1DC826DA107A988D8F70B5D10EAD370EEA5E75D0
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ zigchaind query wasm contract-state smart $CONTRACT_ADDRESS \
  '{"get_pool": {}}' \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz
data:
  total_borrowed: "0"
  total_lent: "20000000"
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ zigchaind query wasm contract-state smart $CONTRACT_ADDRESS \
  '{"get_user": {"address": "zig1ug335mpcdn2vpk8p08v4k9z7cqtdg0jj4tqr92"}}' \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz
data:
  borrowed: "0"
  lent: "20000000"
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ zigchaind tx wasm execute $CONTRACT_ADDRESS   '{"deposit": {}}'   --amount 80000000uzig   --from test-wallet   --gas auto --gas-adjustment 1.3 --gas-prices 0.0025uzig   --chain-id zig-test-
2   --node https://public-zigchain-testnet-rpc.numia.xyz   -y
gas estimate: 170604
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
txhash: 38B4FFF6A9216D1F4777B847EEDB37D16F1431247E59B2B180060C2B5CBC3CE7
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ zigchaind query wasm contract-state smart $CONTRACT_ADDRESS   '{"get_user": {"address": "zig1ug335mpcdn2vpk8p08v4k9z7cqtdg0jj4tqr92"}}'   --chain-id zig-test-2   --node https://public-zigchain-testnet-rpc.numia.xyz
data:
  borrowed: "0"
  lent: "100000000"
  shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$  zigchaind q bank balances wallet2
balances:
- amount: "4480458"
  denom: uzig
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ zigchaind tx wasm execute $CONTRACT_ADDRESS \
  '{"borrow": {"amount": "100000000"}}' \
  --from wallet2 \
  --gas auto --gas-adjustment 1.3 --gas-prices 0.0025uzig \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz \
  -y
gas estimate: 166242
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
txhash: 5AAADCFC756ED779FD8CD8BE60788F11758FA0A63B8BF0F131E9E1BDFC31EE05
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ zigchaind query wasm contract-state smart $CONTRACT_ADDRESS \
  '{"get_pool": {}}' \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz
data:
  total_borrowed: "100000000"
  total_lent: "100000000"
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ zigchaind query wasm contract-state smart $CONTRACT_ADDRESS \
  '{"get_user": {"address": "zig1ug335mpcdn2vpk8p08v4k9z7cqtdg0jj4tqr92"}}' \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz
data:
  borrowed: "0"
  lent: "100000000"
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ zigchaind query wasm contract-state smart $CONTRACT_ADDRESS   '{"get_user": {"address": "zig1jkdwa6a5yjur950nvg3t4n6xh4dau4t4sphckq"}}'   --chain-id zig-test-2   --node https://public-zigch
ain-testnet-rpc.numia.xyz
data:
  borrowed: "100000000"
  lent: "0"
 shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$  zigchaind q bank balances wallet2
balances:
- amount: "1448041"
  denom: uzig
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$  zigchaind q bank balances test-wallet
balances:
- amount: "521709745"
  denom: uzig
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ zigchaind tx wasm execute $CONTRACT_ADDRESS \
  '{"repay": {}}' \
  --amount 100000000uzig \
  --from wallet2 \
  --gas auto --gas-adjustment 1.3 --gas-prices 0.0025uzig \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz \
  -y
rpc error: code = Unknown desc = rpc error: code = Unknown desc = failed to execute message; message index: 0: Insufficient funds: execute wasm contract failed [!cosm!wasm/wasmd@v0.55.1/x/wasm/keeper/keeper.go:444] with gas used: '120432': unknown request
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ zigchaind tx wasm execute $CONTRACT_ADDRESS   '{"repay": {}}'   --amount 105000000uzig   --from wallet2   --gas auto --gas-adjustment 1.3 --gas-prices 0.0025uzig   --chain-id zig-test-2   -
-node https://public-zigchain-testnet-rpc.numia.xyz   -y
gas estimate: 165058
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
txhash: 1928FEA65D2EBC3C325A92C824383D83BDAC5375238C212F82CDAD58F515591E
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ zigchaind query wasm contract-state smart $CONTRACT_ADDRESS   '{"get_user": {"address": "zig1jkdwa6a5yjur950nvg3t4n6xh4dau4t4sphckq"}}'   --chain-id zig-test-2   --node https://public-zigchain-testnet-rpc.numia.xyz
data:
  borrowed: "0"
  lent: "0"
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ zigchaind query wasm contract-state smart $CONTRACT_ADDRESS   '{"get_user": {"address": "zig1ug335mpcdn2vpk8p08v4k9z7cqtdg0jj4tqr92"}}'   --chain-id zig-test-2   --node https://public-zigch
ain-testnet-rpc.numia.xyz
data:
  borrowed: "0"
  lent: "105000000"
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$ zigchaind tx wasm execute $CONTRACT_ADDRESS \
  '{"withdraw": {"amount": "105000000"}}' \
  --from test-wallet \
  --gas auto --gas-adjustment 1.3 --gas-prices 0.0025uzig \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz \
  -y
gas estimate: 165116
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
txhash: DC1562AAE6E82B633C5242617A123CDB26EB6F3642B61FB4D4486CB89A7C9A03
shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/simple-lending-borrowing/simple-lending-borrowing$  zigchaind q bank balances test-wallet
balances:
- amount: "626709745"
  denom: uzig

shahnil@DESKTOP-SKSN1G6:~/Desktop_extracted/Desktop/NewFolder/lp-transfer/liquidity-protocol$ zigchaind q bank balances wallet2
balances: 
- amount: "39803760"
  denom: uzig



















# Simple Lending & Borrowing Contract API

This document describes the messages supported by the `simple-lending-borrowing` contract on ZIGChain.

## InstantiateMsg
Used to initialize the contract.
- `denom`: The native token denomination used for lending and borrowing (e.g., `"uzig"`).
- `interest_rate`: The fixed interest rate for loans, scaled by $1,000,000$ (e.g., `50000` for 5%).

## ExecuteMsg

### `Deposit {}`
Lend funds to the contract.
- **Requirement**: Must send funds matching the contract's `denom`.
- **Action**: Issues pool shares to the sender based on the current pool value.
- **Yield**: Liquidity providers earn yield as interest is paid back by borrowers.

**CLI Example:**
```bash
zigchaind tx wasm execute $CONTRACT_ADDRESS \
  '{"deposit": {}}' \
  --amount 1000000uzig \
  --from $WALLET_ID \
  --gas auto --gas-adjustment 1.3 --gas-prices 0.0025uzig \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz \
  -y
```

### `Withdraw { amount: Uint128 }`
Withdraw lent funds and accumulated interest.
- **Parameter**: `amount` of assets to withdraw.
- **Action**: Calculates required shares to burn and sends assets back to the lender.
- **Requirement**: Contract must have sufficient available liquidity (total lent - total borrowed).

**CLI Example:**
```bash
zigchaind tx wasm execute $CONTRACT_ADDRESS \
  '{"withdraw": {"amount": "1000000"}}' \
  --from $WALLET_ID \
  --gas auto --gas-adjustment 1.3 --gas-prices 0.0025uzig \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz \
  -y
```

### `Borrow { amount: Uint128 }`
Borrow funds from the pool.
- **Parameter**: `amount` to borrow.
- **Action**: Transfers the requested amount to the sender and records the debt.
- **Requirement**: The pool must have enough unborrowed liquidity.

**CLI Example:**
```bash
zigchaind tx wasm execute $CONTRACT_ADDRESS \
  '{"borrow": {"amount": "500000"}}' \
  --from $WALLET_ID \
  --gas auto --gas-adjustment 1.3 --gas-prices 0.0025uzig \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz \
  -y
```

### `Repay {}`
Repay a loan with interest.
- **Requirement**: Must send funds covering the principal plus calculated interest.
- **Action**: Clears the borrower's debt and adds the interest portion to the pool's assets.
- **Yield Generation**: The interest portion increases the value of pool shares for all lenders.
- **Note**: Any overpayment is returned to the sender.

**CLI Example:**
```bash
zigchaind tx wasm execute $CONTRACT_ADDRESS \
  '{"repay": {}}' \
  --amount 525000uzig \
  --from $WALLET_ID \
  --gas auto --gas-adjustment 1.3 --gas-prices 0.0025uzig \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz \
  -y
```

## QueryMsg

### `GetPool {}`
Returns the current state of the global lending pool.
- **Returns**: `PoolResponse`
    - `total_lent`: Total assets "owned" by the pool (available + out on loan).
    - `total_borrowed`: Assets currently out with borrowers.

**CLI Example:**
```bash
zigchaind query wasm contract-state smart $CONTRACT_ADDRESS \
  '{"get_pool": {}}' \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz
```

### `GetUser { address: String }`
Returns the lending and borrowing status of a specific user.
- **Parameter**: `address` (ZIGChain address string).
- **Returns**: `UserResponse`
    - `lent`: The current value of the user's position in `uzig` (original deposit + earned interest).
    - `borrowed`: The user's current outstanding principal.

**CLI Example:**
```bash
zigchaind query wasm contract-state smart $CONTRACT_ADDRESS \
  '{"get_user": {"address": "zig1..."}}' \
  --chain-id zig-test-2 \
  --node https://public-zigchain-testnet-rpc.numia.xyz
```
