# Getting Started with Token Vault

This guide will walk you through using the Token Vault from start to finish. Whether you're depositing stablecoin for the first time or withdrawing your liquidity, this document covers everything you need to know.

## What is Token Vault?

Token Vault is a liquidity protocol that accepts stablecoin deposits and issues LP (Liquidity Provider) tokens in return. The system maintains a 1:1 ratio, meaning 1 stablecoin deposited gives you 1 LP token, and burning 1 LP token returns 1 stablecoin.

**Key Benefits:**
- No slippage on deposits or withdrawals
- No fees for holding LP tokens
- LP tokens are native tokens that work with all wallets
- Secure smart contract with built-in safety checks
- Simple and transparent operations

## Initial Setup

Before you can interact with the vault, you need a few things:

### 1. Have a Wallet with Funds

You need a wallet on ZigChain testnet with some UZIG tokens. If you don't have a wallet yet, create one:

```bash
zigchaind keys add mywallet
```

Make sure your wallet has UZIG tokens for:
- Deposits (the amount you want to deposit)
- Gas fees (usually around 0.3 UZIG per transaction)

### 2. Locate the Contract Address

The current vault contract address is saved in `scripts/vault_addresses.txt`. You can view it:

```bash
cat scripts/vault_addresses.txt
```

This file contains:
- Contract address
- RPC node URL
- Your wallet address
- LP token denomination

### 3. Test Your Connection

Before doing anything, verify you can connect to the network:

```bash
zigchaind status --node https://public-zigchain-testnet-rpc.numia.xyz:443
```

If this returns network information, you're connected and ready to proceed.

## Using the Interactive Script

The easiest way to interact with the vault is using the provided interactive script. This script provides a menu-driven interface for all operations.

### Starting the Script

```bash
cd liquidity-protocol
bash scripts/interact_tokenfactory.sh
```

The script will show you a menu with these options:

```
1) Deposit Stablecoin
2) Withdraw Stablecoin
3) Query Your Balance
4) Query Your Position
5) Query Vault State
6) Query Config
7) Exit
```

### Option 1: Deposit Stablecoin

This option lets you deposit UZIG and receive LP tokens.

**Step-by-step process:**

1. Select option `1` from the menu
2. The script shows your current balances:
   ```
   Your current balances:
   - UZIG: 1843626105 uzig
   - LP tokens: 52150
   ```
3. Enter the amount you want to deposit (in uzig, not UZIG)
   - 1 UZIG = 1,000,000 uzig
   - So to deposit 10 UZIG, enter `10000000`
4. Confirm the transaction when prompted
5. Wait for confirmation (usually 5-10 seconds)
6. The script shows your new balances

**Example:**
```
Enter amount to deposit (in uzig): 10000000
Depositing 10000000 uzig...

Transaction successful!
Hash: A1B2C3D4E5F6...

Your new balances:
- UZIG: 1833626105 uzig
- LP tokens: 10052150
```

**What happens behind the scenes:**
- Your UZIG is transferred to the vault contract
- The contract mints LP tokens equal to your deposit
- LP tokens are sent to your wallet
- Your balance is recorded in the contract

### Option 2: Withdraw Stablecoin

This option lets you burn LP tokens and get your UZIG back.

**Step-by-step process:**

1. Select option `2` from the menu
2. The script shows your LP token balance
3. Enter the amount of UZIG you want to receive
   - This must be less than or equal to your LP balance
4. Confirm the transaction
5. Wait for confirmation
6. The script shows your updated balances

**Example:**
```
Your LP balance: 10052150

Enter amount to withdraw (in uzig): 5000000
Withdrawing 5000000 uzig...

Transaction successful!
Hash: F6E5D4C3B2A1...

Your new balances:
- UZIG: 1838626105 uzig
- LP tokens: 5052150
```

**What happens behind the scenes:**
- LP tokens are burned from your wallet
- The contract transfers UZIG back to you
- Your balance in the contract is updated
- Total vault liquidity decreases

### Option 3: Query Your Balance

Quick check of your LP token balance. This is free and doesn't require gas.

**Output:**
```
Your LP token balance: 5052150
```

### Option 4: Query Your Position

Detailed view of your position in the vault, including the stablecoin value.

**Output:**
```
Address: zig1ug335mpcdn2vpk8p08v4k9z7cqtdg0jj4tqr92
LP Balance: 5052150
Stablecoin Value: 5052150
```

### Option 5: Query Vault State

See the overall state of the vault including total deposits and LP supply.

**Output:**
```
Total Stablecoin Deposited: 52150
Total LP Supply: 52150
```

This tells you:
- How much liquidity is in the vault
- Total LP tokens in circulation
- Whether the 1:1 ratio is maintained

### Option 6: Query Config

Technical information about the contract configuration.

**Output:**
```
Stablecoin Denom: uzig
LP Token Denom: coin.zig1jj787thux4fycfh9882lelpzqmxfht5jq62r54x5x59lpr4wuvtqgmdcvm.vaulttoken
Admin: zig1ug335mpcdn2vpk8p08v4k9z7cqtdg0jj4tqr92
```

## Understanding LP Tokens

LP tokens are your proof of deposit. Here's what you need to know:

### They're Native Tokens
LP tokens use TokenFactory, which means they're native blockchain tokens just like UZIG. This has several advantages:
- They appear in your normal wallet balance
- You can send them to other wallets
- They work with all wallets and exchanges
- No special contract interaction needed to check balance

### They're Transferable
You can send LP tokens to anyone:
```bash
zigchaind tx bank send mywallet recipient_address 1000000vaulttoken --node $NODE
```

The recipient can then use those LP tokens to withdraw stablecoin from the vault.

### They Represent Your Claim
Each LP token represents a claim on 1 unit of stablecoin in the vault. If you have 10,000,000 LP tokens, you can withdraw 10,000,000 uzig (10 UZIG).

### They Don't Expire
LP tokens have no expiration date. You can hold them indefinitely and withdraw whenever you want, as long as the vault has liquidity.

## Common Workflows

### First-Time Deposit

1. Check your UZIG balance: Select option 3
2. Check the vault state: Select option 5
3. Deposit UZIG: Select option 1, enter amount
4. Verify LP tokens received: Select option 4

### Regular Withdrawal

1. Check your LP balance: Select option 3
2. Check vault has liquidity: Select option 5
3. Withdraw stablecoin: Select option 2, enter amount
4. Verify UZIG received: Check your wallet balance

### Partial Withdrawal

You don't have to withdraw everything at once:

1. Current LP balance: 10,000,000
2. Withdraw 3,000,000 uzig
3. Remaining LP balance: 7,000,000
4. Can withdraw the remaining 7,000,000 later

### Monitor Your Position

If you want to track your position over time:

1. Use option 4 to query your position
2. Note your LP balance and stablecoin value
3. Check periodically to see if anything changed
4. All queries are free (no gas fees)

## Important Considerations

### Gas Fees

Each deposit or withdrawal transaction requires gas. Typical costs:
- Deposit: ~0.27 UZIG
- Withdrawal: ~0.29 UZIG
- Queries: Free (no gas required)

Make sure you always have extra UZIG in your wallet for gas fees.

### Network Delays

Transactions take time to process:
- Fast blocks: 5-10 seconds
- Network congestion: 30-60 seconds
- Query updates: Immediate after block confirmation

Don't panic if your balance doesn't update instantly. Wait for the transaction to be included in a block.

### Minimum Amounts

There's no strict minimum, but very small amounts are impractical:
- Minimum deposit: Technically 1 uzig, but gas costs more
- Practical minimum: 1,000,000 uzig (1 UZIG)
- Maximum: Limited by your balance and contract limits

### Liquidity Requirements

Withdrawals require the vault to have sufficient liquidity:
- Check vault state before large withdrawals
- If vault is empty, you must wait for deposits
- Your LP tokens remain valid until you can withdraw

### Safety Checks

The contract performs several checks on every transaction:
- You can't deposit 0 amount
- You can't withdraw more than you have
- Only UZIG is accepted (other tokens rejected)
- All math is overflow-protected
- Balances are verified at each step

## Troubleshooting

### "Insufficient funds" error

**Problem:** You don't have enough UZIG or LP tokens

**Solution:**
- For deposits: Check your UZIG balance
- For withdrawals: Check your LP token balance
- Make sure you account for gas fees

### "Invalid amount" error

**Problem:** You entered 0 or a negative amount

**Solution:** Enter a positive amount greater than 0

### Transaction pending forever

**Problem:** Network congestion or RPC issues

**Solution:**
- Wait up to 2 minutes
- Check transaction hash on block explorer
- Try different RPC node if needed

### Balance not updating

**Problem:** Query returns old information

**Solution:**
- Wait 10-15 seconds after transaction
- Query again
- Check transaction was confirmed

### Script says "configuration not found"

**Problem:** Missing or corrupted `vault_addresses.txt`

**Solution:**
- Check the file exists in `scripts/` folder
- Verify it contains all required variables
- Re-run deployment script if needed

## Next Steps

Now that you understand the basics:

1. **Try a small deposit** - Start with 1-5 UZIG to test
2. **Check your position** - Verify LP tokens received
3. **Try a withdrawal** - Withdraw half to test the process
4. **Read the security docs** - See `docs/SECURITY.md`
5. **Learn about queries** - See `docs/QUERIES.md` for advanced usage

## Getting Help

If you encounter issues:

1. Check the troubleshooting section above
2. Review the security documentation
3. Verify your wallet has sufficient funds
4. Check the blockchain explorer for your transactions
5. Ensure you're using the correct contract address

The vault is designed to be simple and safe. Most issues are related to insufficient balances or network connectivity rather than contract problems.

---

**Quick Reference Commands:**

```bash
# Run interactive script
bash scripts/interact_tokenfactory.sh

# Check configuration
cat scripts/vault_addresses.txt

# View your wallet balances
zigchaind query bank balances $(zigchaind keys show mywallet -a) --node $NODE

# Test connection
zigchaind status --node $NODE
```

For detailed query documentation, see [QUERIES.md](QUERIES.md).  
For security information, see [SECURITY.md](SECURITY.md).
