# Admin-Managed Yield Vault - Query & Execute Reference

## Overview

This vault contract provides time-locked withdrawal functionality with **admin-managed yield generation**. The admin has exclusive rights to move vault funds to external yield protocols, while users receive proportional yield through the price-per-share mechanism.

### Key Features

- **Admin-Only Fund Management**: Only the admin can move stablecoin to/from yield protocols
- **Automatic Yield Distribution**: Yield is distributed to all LP token holders via increasing price per share
- **Time-Locked Withdrawals**: 2-step withdrawal process with configurable delay (2 minutes - 30 days)
- **Secure Accounting**: Preserves 1:1 accounting between vault value and LP token supply

## Quick Start

```bash
# Load configuration
source scripts/vault_addresses.txt

# Query vault state
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"vault_info":{}}' --node $NODE --output json | jq '.data'

# Query your position
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"user_info\":{\"address\":\"$MY_ADDR\"}}" --node $NODE --output json | jq '.data'

# Query contract configuration
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"config":{}}' --node $NODE --output json | jq '.data'
```

---

## Query Messages

### 1. Config

Returns contract configuration including admin address and withdrawal delay.

**Query:**
```bash
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"config":{}}' --node $NODE --output json | jq '.data'
```

**Response:**
```json
{
  "stablecoin_denom": "uzig",
  "lp_full_denom": "coin.zig1abc.../lp",
  "admin": "zig1admin...",
  "withdrawal_delay": 172800
}
```

**Fields:**
- `stablecoin_denom`: The deposit token denomination (e.g., "uzig")
- `lp_full_denom`: Full LP token denomination created by TokenFactory
- `admin`: Admin address with vault management privileges
- `withdrawal_delay`: Time lock duration in seconds (IMMUTABLE)

---

### 2. Vault Info

Returns total vault value, LP supply, and current price per share.

**Query:**
```bash
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  '{"vault_info":{}}' --node $NODE --output json | jq '.data'
```

**Response:**
```json
{
  "total_deposited": "1050000",
  "total_lp_supply": "1000000",
  "total_pending_withdrawals": "100000",
  "price_per_share": "1.050000"
}
```

**Fields:**
- `total_deposited`: Total stablecoin value in vault (includes yield)
- `total_lp_supply`: Total LP tokens in circulation
- `total_pending_withdrawals`: Amount locked in time-locked withdrawals
- `price_per_share`: Current price per LP token (grows with yield)

**Price Per Share Formula:**
```
price_per_share = total_deposited / total_lp_supply
```

As yield accrues and admin updates vault value, price per share increases, benefiting all LP holders.

---

### 3. User Info

Returns user's LP token balance and equivalent stablecoin value.

**Query:**
```bash
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"user_info\":{\"address\":\"$USER_ADDRESS\"}}" --node $NODE --output json | jq '.data'
```

**Response:**
```json
{
  "address": "zig1user...",
  "lp_balance": "100000",
  "stablecoin_value": "105000"
}
```

**Fields:**
- `address`: User's wallet address
- `lp_balance`: Amount of LP tokens owned
- `stablecoin_value`: Equivalent stablecoin value (includes yield)

---

### 4. Pending Withdrawals

Returns all pending time-locked withdrawals for a user.

**Query:**
```bash
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"pending_withdrawals\":{\"address\":\"$USER_ADDRESS\"}}" --node $NODE --output json | jq '.data'
```

**Response:**
```json
{
  "address": "zig1user...",
  "withdrawals": [
    {
      "id": 0,
      "amount": "50000",
      "release_time": "1706745600",
      "claimable": false
    },
    {
      "id": 1,
      "amount": "25000",
      "release_time": "1706659200",
      "claimable": true
    }
  ]
}
```

**Fields:**
- `id`: Withdrawal ID (unique per user)
- `amount`: Stablecoin amount (includes yield at request time)
- `release_time`: Unix timestamp when withdrawal becomes claimable
- `claimable`: Whether withdrawal can be claimed now

---

### 5. Specific Withdrawal

Query details of a single pending withdrawal.

**Query:**
```bash
zigchaind query wasm contract-state smart $VAULT_ADDRESS \
  "{\"withdrawal\":{\"address\":\"$USER_ADDRESS\",\"withdrawal_id\":0}}" \
  --node $NODE --output json | jq '.data'
```

**Response:**
```json
{
  "address": "zig1user...",
  "withdrawal": {
    "id": 0,
    "amount": "50000",
    "release_time": "1706745600",
    "claimable": false
  }
}
```

---

## Execute Messages

### User Operations

#### 1. Deposit

Deposit stablecoin to receive LP tokens.

**Command:**
```bash
zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"deposit":{}}' \
  --from $WALLET \
  --amount 1000uzig \
  --node $NODE \
  --gas auto --gas-adjustment 1.5 \
  -y
```

**Calculation:**
- **First deposit**: Mint 1:1 (1000 ZIG → 1000 LP tokens)
- **Subsequent deposits**: `lp_to_mint = deposit_amount * total_lp_supply / total_deposited`

#### 2. Request Withdraw

Burns LP tokens and creates time-locked withdrawal request.

**Command:**
```bash
zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"request_withdraw":{}}' \
  --from $WALLET \
  --amount 500coin.zig1abc.../lp \
  --node $NODE \
  --gas auto --gas-adjustment 1.5 \
  -y
```

**Process:**
1. Burns LP tokens immediately
2. Calculates stablecoin amount: `amount = lp_tokens * total_deposited / total_lp_supply`
3. Creates pending withdrawal with time lock
4. Returns withdrawal ID

**Note:** Check response attributes for withdrawal_id.

#### 3. Claim Withdraw

Claims a time-locked withdrawal after delay expires.

**Command:**
```bash
zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"claim_withdraw":{"withdrawal_id":0}}' \
  --from $WALLET \
  --node $NODE \
  --gas auto --gas-adjustment 1.5 \
  -y
```

**Requirements:**
- Time lock must have expired
- Vault must have sufficient stablecoin balance
- Admin must ensure liquidity if funds are in yield protocols

---

### Admin Operations

#### 4. Admin Withdraw

**Admin-only**: Withdraw stablecoin from vault to admin's wallet for external management.

**Command:**
```bash
zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"admin_withdraw":{"amount":"1000000"}}' \
  --from $ADMIN_WALLET \
  --node $NODE \
  --gas auto --gas-adjustment 1.5 \
  -y
```

**Purpose:**
- Admin withdraws funds from vault to manage externally
- Admin generates yield OFF-CHAIN (lending, staking, trading, etc.)
- Funds are sent to admin's wallet only (not to arbitrary addresses)
- Does NOT change vault accounting (total_deposited unchanged)

**Security:**
- Only admin can call this
- Withdraws to admin wallet only (no arbitrary addresses)
- Check vault has sufficient balance
- Admin responsible for depositing funds + yield back

**Example:**
```
Initial vault: 1,000,000 ZIG
Admin withdraws: 800,000 ZIG
  → Vault now has 200,000 ZIG physical balance
  → total_deposited still 1,000,000 (accounting unchanged)
  → price_per_share still 1.0

Admin manages 800,000 ZIG externally:
  - Lends on external protocol
  - Earns 50,000 ZIG yield
  - Total: 850,000 ZIG (800k principal + 50k yield)

Admin must deposit back via AdminDepositYield
```

#### 5. Admin Deposit Yield

**Admin-only**: Deposit yield earned from external protocols directly into the vault.

**Command:**
```bash
zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"admin_deposit_yield":{}}' \
  --from $ADMIN_WALLET \
  --amount 50000uzig \
  --node $NODE \
  --gas auto --gas-adjustment 1.5 \
  -y
```

**Purpose:**
- **Single-step yield deposit**: Admin sends stablecoin (yield) directly to contract
- **Automatic accounting**: Contract automatically increases total_deposited
- **No LP tokens minted**: Only increases price per share
- **All LP holders benefit proportionally**: Yield is distributed through price increase

**How It Works:**
1. Admin sends stablecoin funds via `--amount` flag
2. Contract validates admin-only access
3. Contract adds deposit amount to `total_deposited`
4. Price per share increases automatically
5. No LP tokens are created

**Benefits vs Old Approach:**
- ✅ Single atomic transaction (was: 2-step process)
- ✅ On-chain proof of yield deposit (auditable)
- ✅ No manual bookkeeping errors
- ✅ Automatic state updates

**Validation:**
- Only admin can call this function
- Must send exactly one token type (stablecoin)
- Amount must be > 0
- Uses checked arithmetic (overflow protection)

**Example:**
```
Initial state:
  total_deposited = 1,000,000
  total_lp_supply = 1,000,000
  price_per_share = 1.0

Admin deposits 50,000 yield:
  zigchaind tx wasm execute $VAULT --amount 50000uzig ...
  
New state:
  total_deposited = 1,050,000  // Automatically increased
  total_lp_supply = 1,000,000  // Unchanged (no LP minted)
  price_per_share = 1.05       // 5% increase

User with 100,000 LP tokens:
  Before: 100,000 LP * 1.0 = 100,000 ZIG
  After:  100,000 LP * 1.05 = 105,000 ZIG
  Yield earned: 5,000 ZIG (5% of their position)
```

**Security:**
- ✅ Admin-only access control
- ✅ Funds must be actually transferred (cryptographic proof)
- ✅ Automatic state consistency
- ✅ No LP dilution (supply unchanged)


#### 6. Update Config

**Admin-only**: Update stablecoin denom or admin address.

**Command:**
```bash
zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"update_config":{"admin":"zig1new_admin..."}}' \
  --from $ADMIN_WALLET \
  --node $NODE \
  --gas auto --gas-adjustment 1.5 \
  -y
```

**Updateable Fields:**
- `stablecoin_denom`: Change deposit token (optional)
- `admin`: Transfer admin rights (optional)

**IMMUTABLE Field:**
- `withdrawal_delay`: Cannot be changed after instantiation (security)

---

## Admin Workflow Example

### Complete Yield Generation Cycle

**1. User deposits funds:**
```bash
# User deposits 1000 ZIG
zigchaind tx wasm execute $VAULT_ADDRESS '{"deposit":{}}' \
  --from user --amount 1000uzig --node $NODE -y
```

**2. Admin withdraws funds for external management:**
```bash
# Admin withdraws 800 ZIG to admin wallet
zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"admin_withdraw":{"amount":"800"}}' \
  --from admin --node $NODE -y
```

**3. Admin generates yield OFF-CHAIN:**
```
Admin manages 800 ZIG externally (lending, staking, DeFi strategies, etc.)
Time passes... admin earns 50 ZIG yield
Total: 800 ZIG principal + 50 ZIG yield = 850 ZIG
```

**4. Admin deposits yield to vault (single step):**
```bash
# Admin sends 50 ZIG yield directly to vault
# This automatically increases total_deposited and price per share
zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"admin_deposit_yield":{}}' \
  --from admin --amount 50uzig --node $NODE -y
```

**5. Price per share increases:**
```
Before: 1000 deposited / 1000 LP = 1.0 per share
After:  1050 deposited / 1000 LP = 1.05 per share
```

**6. User withdraws (gets yield automatically):**
```bash
# User requests withdrawal of 1000 LP tokens
# Gets 1000 * 1.05 = 1050 ZIG (including 50 ZIG yield)
zigchaind tx wasm execute $VAULT_ADDRESS '{"request_withdraw":{}}' \
  --from user --amount 1000coin.zig1.../lp --node $NODE -y
```

**7. Admin ensures liquidity:**
```bash
# Before claim period, admin brings funds back if needed
# (manual withdrawal from yield protocol + send to vault)
```

**8. User claims after time lock:**
```bash
# After withdrawal delay expires
zigchaind tx wasm execute $VAULT_ADDRESS \
  '{"claim_withdraw":{"withdrawal_id":0}}' \
  --from user --node $NODE -y
```

---

## Contract Instantiation

### Required Parameters

```bash
zigchaind tx wasm instantiate $CODE_ID '{
  "stablecoin_denom": "uzig",
  "lp_subdenom": "vaultlp",
  "lp_minting_cap": "1000000000000",
  "can_change_minting_cap": false,
  "withdrawal_delay_seconds": 172800,
  "admin": "zig1admin...",
  "description": "Admin-Managed Yield Vault LP Token"
}' \
  --from $WALLET \
  --label "admin-vault-v1" \
  --admin $ADMIN_ADDR \
  --node $NODE \
  --gas auto --gas-adjustment 1.5 \
  -y
```

### Parameter Rules

| Parameter | Requirement | Description |
|-----------|-------------|-------------|
| `stablecoin_denom` | Required | Deposit token (e.g., "uzig") |
| `lp_subdenom` | 3-44 chars, lowercase start | LP token subdenom |
| `lp_minting_cap` | > 0 | Maximum LP supply |
| `withdrawal_delay_seconds` | 120 - 2,592,000 | Time lock (2 min - 30 days), **IMMUTABLE** |
| `admin` | Optional | Admin address (defaults to sender) |

**Removed Parameter:**
- `yield_contract_address`: ❌ No longer required. Admin manages funds manually.

---

## Security Considerations

### Admin Responsibilities

1. **Fund Management**: Admin must track funds moved to yield protocols
2. **Liquidity Management**: Admin must ensure sufficient balance for withdrawals
3. **Honest Reporting**: Admin must accurately report yield via `admin_update_vault_value`
4. **Timely Operations**: Admin should bring funds back before users claim withdrawals

### User Protections

1. **Time Lock**: Prevents immediate withdrawal, protecting against flash attacks
2. **Proportional Yield**: All users benefit equally from yield (via price per share)
3. **Immutable Delay**: Withdrawal delay cannot be changed after instantiation
4. **Balance Check**: Claims fail if vault lacks liquidity (protects accounting)

### Invariants

-  **Accounting Integrity**: `total_deposited >= total_lp_supply * price_per_share`
- **Pending Tracking**: `total_pending_withdrawals` tracks locked funds
- **No Value Decrease**: Admin cannot decrease `total_deposited` (only increase)

---

## Error Handling

### Common Errors

**Unauthorized**
```
Error: Only admin can perform this action
```
- Solution: Use admin wallet for admin-only operations

**InsufficientContractBalance**
```
Error: contract has X but needs Y
```
- Solution: Admin must bring funds back from yield protocols before users claim

**WithdrawalLocked**
```
Error: cannot claim until {timestamp}
```
- Solution: Wait for time lock to expire

**InvalidVaultValueDecrease**
```
Error: new total must be >= current total
```
- Solution: Only increase vault value (report yield, don't decrease)

---

## Integration Examples

### Calculate Expected LP Tokens

```python
def calculate_lp_tokens(deposit_amount, total_deposited, total_lp_supply):
    if total_lp_supply == 0 or total_deposited == 0:
        return deposit_amount  # 1:1 for first deposit
    return deposit_amount * total_lp_supply / total_deposited

# Example:
# vault has 1,000,000 deposited, 1,000,000 LP supply
# user deposits 100,000
lp_minted = calculate_lp_tokens(100000, 1000000, 1000000)
# Result: 100,000 LP tokens
```

### Calculate Withdrawal Amount

```python
def calculate_withdrawal_amount(lp_tokens, total_deposited, total_lp_supply):
    if total_lp_supply == 0:
        return 0
    return lp_tokens * total_deposited / total_lp_supply

# Example:
# vault has 1,050,000 deposited (1M + 50K yield), 1,000,000 LP supply
# user burns 100,000 LP
amount = calculate_withdrawal_amount(100000, 1050000, 1000000)
# Result: 105,000 ZIG (includes 5,000 yield)
```

---

## Support & Documentation

- **Project Docs**: See [docs/](docs/) folder for detailed guides
- **Deployment**: See [DEPLOYMENT.md](DEPLOYMENT.md)
- **Architecture**: See [PROJECT_SUMMARY.md](PROJECT_SUMMARY.md)
- **Price Calculation**: See [dlt/PRICECAL.MD](dlt/PRICECAL.MD)

For issues or questions, refer to the project repository.
