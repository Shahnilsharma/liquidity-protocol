# Security Features & Edge Case Handling

This document explains the security measures implemented in the Token Vault contract and how it handles various edge cases. Understanding these protections helps you use the vault with confidence.

## Security Architecture

The Token Vault is built with multiple layers of security to protect user funds and maintain system integrity.

### 1. Input Validation

Every transaction input is validated before processing.

**Amount Validation:**
- All amounts must be greater than zero
- Deposits of 0 uzig are rejected
- Withdrawals of 0 LP tokens are rejected
- Negative amounts are impossible due to type system (Uint128)

**Address Validation:**
- All addresses are validated against bech32 format
- Invalid addresses cause immediate transaction failure
- Contract verifies sender address matches expected format

**Token Denomination Validation:**
- Only the configured stablecoin denom is accepted
- Sending wrong tokens results in immediate rejection
- LP token denom is verified on withdrawals

**Why this matters:** Prevents spam transactions, maintains clean state, and protects against accidental mistakes.

### 2. Balance Verification

The contract cross-checks balances at multiple levels.

**Before Deposits:**
- Verifies user sent the claimed amount
- Checks funds actually arrived in contract
- Confirms bank module balance matches expected

**Before Withdrawals:**
- Queries user's LP token balance from bank module
- Verifies user has sufficient LP tokens
- Confirms contract has sufficient stablecoin to pay out

**After All Operations:**
- Validates total LP supply equals total deposits
- Ensures 1:1 ratio is maintained
- Checks internal state matches bank balances

**Why this matters:** Prevents discrepancies between contract state and actual balances. Even if contract state gets corrupted, bank module balances are authoritative.

### 3. Arithmetic Safety

All mathematical operations use checked arithmetic.

**Overflow Protection:**
- Addition operations check for overflow
- Multiplication operations verify results
- If overflow would occur, transaction fails

**Underflow Protection:**
- Subtraction operations check for underflow
- Withdrawals verify sufficient balance exists
- Division operations avoid division by zero

**Precision Maintenance:**
- All values use Uint128 type
- Maximum value: 340,282,366,920,938,463,463,374,607,431,768,211,455
- No floating point operations (all integer math)

**Why this matters:** Prevents mathematical exploits where attackers might try to wrap numbers around limits to create tokens from nothing or drain the vault.

### 4. Reentrancy Protection

The contract is protected against reentrancy attacks.

**How it works:**
- All state changes happen before external calls
- Funds transfer happens after state update
- No callbacks to user contracts

**Example flow (deposit):**
1. Validate inputs
2. Update user balance in state
3. Update total deposits in state
4. Mint LP tokens to user
5. Record transaction complete

If step 4 fails, the entire transaction reverts including steps 2 and 3. There's no intermediate state where attacker could re-enter and exploit.

**Why this matters:** Prevents attackers from calling deposit/withdraw multiple times in a single transaction to drain funds or mint unlimited tokens.

### 5. Admin Controls

The contract has an admin role with limited permissions.

**Admin CAN:**
- View all contract data (same as everyone else)
- Execute emergency functions if implemented (like pause)

**Admin CANNOT:**
- Withdraw user funds
- Modify user balances
- Change the 1:1 ratio
- Mint or burn tokens arbitrarily
- Transfer ownership without authorization
- Bypass normal deposit/withdrawal logic

**Admin privileges are restricted by design.** The contract code prevents the admin from accessing user funds even if they wanted to.

**Why this matters:** Even if the admin account is compromised, user funds remain safe. The admin can only pause operations, not steal funds.

### 6. State Consistency

The contract maintains invariants that must always be true.

**Core Invariant:**
```
Total LP Supply == Total Stablecoin Deposited
```

This is checked on every operation. If this equation ever becomes false, something has gone seriously wrong.

**Additional Checks:**
- Sum of all user LP balances equals total LP supply
- Contract stablecoin balance equals total deposited
- No user can have negative balance
- No operation can break the 1:1 ratio

**Why this matters:** If any invariant is broken, it's immediately detectable. This makes bugs obvious rather than hidden.

### 7. External Dependency Trust

The contract relies on minimal external dependencies.

**Trusted Components:**
- CosmWasm runtime (audited and battle-tested)
- TokenFactory module (native blockchain feature)
- Bank module (core blockchain component)

**NOT Trusted:**
- User input (always validated)
- External contracts (not used)
- Off-chain data (not used)
- Oracle feeds (not used)

**Why this matters:** Fewer dependencies mean fewer potential vulnerabilities. The contract doesn't rely on external price feeds, oracle data, or other contracts that could be manipulated.

## Edge Case Handling

The contract handles various edge cases gracefully.

### Zero Amount Operations

**Scenario:** User tries to deposit or withdraw 0 tokens

**Handling:**
```
Error: InvalidAmount
Message: "Amount must be greater than zero"
```

**Impact:** Transaction fails immediately, no gas wasted on processing

**Why:** Prevents spam transactions and maintains clean transaction history

### Insufficient Balance

**Scenario:** User tries to withdraw more LP tokens than they have

**Handling:**
```
Error: InsufficientFunds
Message: "Not enough LP tokens"
```

**Impact:** Transaction fails, user balance unchanged

**Why:** Protects against attempted overdrafts and maintains accurate accounting

### Wrong Token Sent

**Scenario:** User sends ATOM, OSMO, or other tokens instead of UZIG

**Handling:**
```
Error: InvalidDenom
Message: "Only uzig accepted"
```

**Impact:** Transaction fails, tokens not accepted

**Why:** Vault only accepts the configured stablecoin. Other tokens would break the 1:1 ratio.

### Vault Empty State

**Scenario:** First user deposits into a brand new vault with 0 balance

**Handling:**
- Deposit processes normally
- User receives LP tokens equal to deposit
- Vault records first deposit correctly
- 1:1 ratio initialized

**Impact:** No special handling needed, works as expected

**Why:** Zero balance is a normal state, not an error condition

### Large Deposits

**Scenario:** User deposits a very large amount (billions of tokens)

**Handling:**
- Transaction processes if amount is within Uint128 limits
- All math operations checked for overflow
- If overflow would occur, transaction fails
- User funds remain safe

**Impact:** Either succeeds completely or fails completely (no partial execution)

**Why:** Protects against overflow exploits while allowing legitimate large deposits

### Dust Amounts

**Scenario:** User withdraws leaving 1-2 uzig worth of LP tokens

**Handling:**
- Small balances remain in user's account
- No minimum balance requirement
- User can withdraw remaining amount anytime
- No penalty for small balances

**Impact:** No restriction on partial withdrawals

**Why:** Users should have full control over their funds, no matter how small

### Multiple Simultaneous Users

**Scenario:** Several users deposit/withdraw at the same time

**Handling:**
- Transactions process sequentially within each block
- Each transaction sees updated state from previous transactions
- No race conditions or conflicts
- Order determined by block validators

**Impact:** All transactions process correctly in order

**Why:** Blockchain consensus ensures consistent ordering

### LP Token Transfers

**Scenario:** User transfers LP tokens to another address

**Handling:**
- Transfer succeeds (LP tokens are normal native tokens)
- Recipient can now withdraw using those tokens
- Original depositor no longer has claim on that amount
- Contract tracking updates automatically

**Impact:** This is a feature, not a bug

**Why:** LP tokens are transferable assets representing vault claims. This enables trading, gifting, and other use cases.

### Network Congestion

**Scenario:** Blockchain is congested with many transactions

**Handling:**
- Transactions wait in mempool until included
- Queries still work instantly (no transaction needed)
- Users may need to pay higher gas fees
- Contract operation unaffected by network state

**Impact:** Slower confirmation times but no data loss

**Why:** This is a network-level issue, not a contract issue

### RPC Node Failure

**Scenario:** The RPC node goes offline or stops responding

**Handling:**
- Transactions fail to broadcast
- User funds remain in wallet (not lost)
- Switch to different RPC node and retry
- Contract state unchanged

**Impact:** Temporary inability to interact, but no loss of funds

**Why:** RPC nodes are just gateways to the blockchain. Contract exists independently.

### Admin Account Compromised

**Scenario:** Attacker gains access to admin private key

**Handling:**
- Attacker cannot withdraw user funds (not permitted by contract)
- Attacker cannot modify balances (not permitted by contract)
- Attacker could pause contract if such functionality exists
- User funds remain locked but safe

**Impact:** Operations might stop, but funds are secure

**Why:** Admin permissions are strictly limited by contract code. Even compromised admin can't steal funds.

### Contract Code Bug

**Scenario:** Theoretical bug in contract logic

**Handling:**
- All transactions are on-chain and auditable
- Invariant checks detect most bugs immediately
- Balance verification catches discrepancies
- Bank module balances are authoritative
- Admin could potentially deploy fix and migrate

**Impact:** Depends on bug severity, but multiple safety nets exist

**Why:** Defense in depth - multiple checks catch issues at different levels

## Security Best Practices for Users

### Verify Before Transacting

Always check balances before deposits or withdrawals:

```bash
# Check stablecoin balance before deposit
zigchaind query bank balances $MY_ADDR --node $NODE

# Check LP balance before withdrawal  
zigchaind query bank balances $MY_ADDR --node $NODE
```

### Verify After Transacting

Confirm transactions succeeded:

```bash
# After deposit, check LP tokens received
zigchaind query bank balances $MY_ADDR --node $NODE

# After withdrawal, check stablecoin received
zigchaind query bank balances $MY_ADDR --node $NODE
```

### Use Test Amounts First

When using the vault for the first time:
1. Deposit a small amount (1-5 UZIG)
2. Verify LP tokens received
3. Withdraw half to test withdrawal
4. Verify stablecoin received
5. Then deposit your full amount

### Keep Private Keys Safe

Standard wallet security applies:
- Never share your mnemonic or private key
- Use hardware wallets for large amounts
- Verify contract addresses before transacting
- Be cautious of phishing attempts

### Monitor Contract State

Regularly check vault health:

```bash
# Check total deposits and LP supply
zigchaind query wasm contract-state smart $CONTRACT \
  '{"vault_info":{}}' --node $NODE
```

If `total_stablecoin_deposited` doesn't equal `total_lp_supply`, something is wrong.

### Use Official Scripts

The provided scripts have safety checks built in:
- `scripts/interact_tokenfactory.sh` validates inputs
- Shows current balances before operations
- Confirms transactions before broadcasting
- Displays clear error messages

### Stay Updated

Check for updates to:
- Contract code (new versions may be deployed)
- Documentation (security advisories)
- RPC endpoints (if default goes offline)

## Attack Vectors & Mitigations

### Flash Loan Attacks

**Attack:** Use borrowed funds to manipulate vault state

**Mitigation:** Not applicable - vault uses 1:1 ratio with no price manipulation possible. Flash loans can't exploit a constant rate.

### Front-Running

**Attack:** See pending deposit and submit higher-gas transaction first

**Mitigation:** Not applicable - deposits don't depend on order or price. Front-running gains nothing.

### Price Oracle Manipulation

**Attack:** Manipulate price feed to exploit vault

**Mitigation:** Not applicable - vault doesn't use price oracles. Rate is fixed at 1:1.

### Overflow/Underflow Exploits

**Attack:** Wrap numbers around to create tokens or drain vault

**Mitigation:** All arithmetic uses checked operations. Overflow/underflow causes transaction failure, not exploitation.

### Reentrancy Attacks

**Attack:** Re-enter contract mid-transaction to exploit state

**Mitigation:** State updates before external calls. No callbacks to user contracts. Atomic transactions.

### Denial of Service

**Attack:** Spam transactions to prevent legitimate users from interacting

**Mitigation:** Gas fees make spam expensive. Blockchain handles transaction ordering fairly.

### Social Engineering

**Attack:** Trick users into sending tokens to wrong address

**Mitigation:** This is a user education issue. Always verify contract addresses. Use official scripts when possible.

## Audit Considerations

If conducting a security audit, focus on:

1. **Arithmetic Operations** - Verify all math uses checked operations
2. **State Consistency** - Confirm invariants are maintained
3. **Balance Verification** - Check bank module balances are authoritative
4. **Input Validation** - Verify all inputs are properly validated
5. **Access Controls** - Confirm admin can't access user funds
6. **Edge Cases** - Test all scenarios documented here
7. **External Calls** - Verify state changes before external interactions
8. **Type Safety** - Confirm no unsafe type conversions

## Conclusion

The Token Vault implements multiple layers of security:
- Input validation prevents bad data
- Balance verification catches discrepancies
- Checked arithmetic prevents overflow exploits
- Reentrancy protection stops recursive attacks
- Limited admin powers prevent internal threats
- State consistency checks detect anomalies

Edge cases are handled gracefully with clear error messages and transaction rollback. The vault is designed to either process transactions completely or fail cleanly with no partial states.

Users should still follow security best practices, but the contract itself provides strong protections against most common attack vectors in DeFi.

---

For usage instructions, see [GETTING_STARTED.md](GETTING_STARTED.md).  
For query documentation, see [QUERIES.md](QUERIES.md).
