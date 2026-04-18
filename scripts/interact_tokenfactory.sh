#!/bin/bash

# ZigChain Token Vault - TokenFactory Edition Interaction Script

set -e

# Configuration
NODE="https://public-zigchain-testnet-rpc.numia.xyz:443"
CHAIN_ID="zig-test-2"
WALLET="wallet2"
GAS_PRICES="0.0025uzig"
GAS_AUTO="--gas auto --gas-adjustment 1.5"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Load contract addresses
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ -f "$SCRIPT_DIR/vault_addresses.txt" ]; then
    source "$SCRIPT_DIR/vault_addresses.txt"
else
    echo "Error: vault_addresses.txt not found in $SCRIPT_DIR. Run deploy_v2_stack.sh first."
    exit 1
fi

# Get wallet address
MY_ADDR=$(zigchaind keys show $WALLET -a)

echo -e "${BLUE}================================${NC}"
echo -e "${BLUE}Token Vault Interaction${NC}"
echo -e "${BLUE}================================${NC}"
echo ""
echo -e "${GREEN}Wallet:${NC} $MY_ADDR"
echo -e "${GREEN}Contract:${NC} $LP_POOL_ADDRESS"
echo -e "${GREEN}LP Denom:${NC} $LP_FULL_DENOM"
echo -e "${GREEN}Stablecoin:${NC} $STABLECOIN_DENOM"
echo ""

# Menu
while true; do
    echo "Choose an action:"
    echo "USER OPERATIONS:"
    echo "1) Deposit (send stablecoin, receive LP tokens)"
    echo "2) Request Withdrawal (send LP tokens, create pending withdrawal)"
    echo "3) Claim Withdrawal (claim pending withdrawal after time lock)"
    echo ""
    echo "QUERIES:"
    echo "4) Query config (view withdrawal delay & settings)"
    echo "5) Query vault info"
    echo "6) Query user info"
    echo "7) Query pending withdrawals"
    echo "8) Check balances"
    echo ""
    echo "ADMIN OPERATIONS:"
    echo "10) Admin: Withdraw funds from vault"
    echo "11) Admin: Deposit yield to vault"
    echo "12) Admin: Update config"
    echo ""
    echo "9) Exit"
    echo ""
    read -p "Enter choice: " choice

    case $choice in
        1)
            echo -e "${YELLOW}Deposit${NC}"
            read -p "Enter amount of $STABLECOIN_DENOM to deposit: " AMOUNT
            
            echo -e "${BLUE}Sending deposit transaction...${NC}"
            TX=$(zigchaind tx wasm execute $LP_POOL_ADDRESS \
                '{"deposit":{}}' \
                --from $WALLET \
                --amount "${AMOUNT}${STABLECOIN_DENOM}" \
                --node $NODE \
                --chain-id $CHAIN_ID \
                --gas-prices $GAS_PRICES \
                $GAS_AUTO \
                --output json \
                -y)
            
            echo "$TX" | jq '.'
            TXHASH=$(echo "$TX" | jq -r '.txhash')
            echo -e "${GREEN}Transaction hash:${NC} $TXHASH"
            echo -e "${YELLOW}Waiting for confirmation...${NC}"
            sleep 6
            
            RESULT=$(zigchaind query tx $TXHASH --node $NODE --output json 2>/dev/null)
            CODE=$(echo "$RESULT" | jq -r '.code')
            
            if [ "$CODE" = "0" ]; then
                echo -e "${GREEN}✓ Deposit successful!${NC}"
                echo -e "${BLUE}Check your LP token balance (option 6)${NC}"
            else
                echo -e "${YELLOW}Transaction result:${NC}"
                echo "$RESULT" | jq '.code, .raw_log'
            fi
            ;;
            
        2)
            echo -e "${YELLOW}Request Withdrawal${NC}"
            read -p "Enter amount of LP tokens to withdraw: " LP_AMOUNT
            
            echo -e "${BLUE}Requesting withdrawal (2-day time lock will apply)...${NC}"
            TX=$(zigchaind tx wasm execute $LP_POOL_ADDRESS \
                '{"request_withdraw":{}}' \
                --from $WALLET \
                --amount "${LP_AMOUNT}${LP_FULL_DENOM}" \
                --node $NODE \
                --chain-id $CHAIN_ID \
                --gas-prices $GAS_PRICES \
                $GAS_AUTO \
                --output json \
                -y)
            
            echo "$TX" | jq '.'
            TXHASH=$(echo "$TX" | jq -r '.txhash')
            echo -e "${GREEN}Transaction hash:${NC} $TXHASH"
            echo -e "${YELLOW}Waiting for confirmation...${NC}"
            sleep 6
            
            RESULT=$(zigchaind query tx $TXHASH --node $NODE --output json 2>/dev/null)
            CODE=$(echo "$RESULT" | jq -r '.code')
            
            if [ "$CODE" = "0" ]; then
                echo -e "${GREEN}✓ Withdrawal request created!${NC}"
                # Extract withdrawal_id from events
                WITHDRAWAL_ID=$(echo "$RESULT" | jq -r '.events[] | select(.type=="wasm") | .attributes[] | select(.key=="withdrawal_id") | .value' | head -1)
                RELEASE_TIME=$(echo "$RESULT" | jq -r '.events[] | select(.type=="wasm") | .attributes[] | select(.key=="release_time") | .value' | head -1)
                
                if [ -n "$WITHDRAWAL_ID" ] && [ "$WITHDRAWAL_ID" != "null" ]; then
                    echo -e "${BLUE}Withdrawal ID:${NC} $WITHDRAWAL_ID"
                    echo -e "${BLUE}Release Time:${NC} $RELEASE_TIME (Unix timestamp)"
                    
                    # Convert timestamp to human readable
                    if command -v date &> /dev/null; then
                        READABLE_TIME=$(date -d @"$RELEASE_TIME" 2>/dev/null || date -r "$RELEASE_TIME" 2>/dev/null || echo "Unable to convert")
                        echo -e "${BLUE}Release Date:${NC} $READABLE_TIME"
                    fi
                    
                    echo -e "${YELLOW}Note: You must wait 2 days (172,800 seconds) before claiming.${NC}"
                    echo -e "${BLUE}Use option 3 to claim when ready, and option 7 to check pending withdrawals.${NC}"
                else
                    echo -e "${BLUE}Check your pending withdrawals (option 7)${NC}"
                fi
            else
                echo -e "${YELLOW}Transaction result:${NC}"
                echo "$RESULT" | jq '.code, .raw_log'
            fi
            ;;
            
        3)
            echo -e "${YELLOW}Claim Withdrawal${NC}"
            
            # First show pending withdrawals
            echo -e "${BLUE}Your pending withdrawals:${NC}"
            PENDING=$(zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
                "{\"pending_withdrawals\":{\"address\":\"$MY_ADDR\"}}" \
                --node $NODE \
                --output json)
            
            echo "$PENDING" | jq '.data'
            
            # Get current time for comparison
            CURRENT_TIME=$(date +%s)
            
            # Extract claimable withdrawals
            CLAIMABLE=$(echo "$PENDING" | jq ".data.withdrawals[] | select(.claimable==true)")
            
            if [ -z "$CLAIMABLE" ]; then
                echo -e "${YELLOW}No withdrawals are ready to claim yet.${NC}"
                echo -e "${BLUE}Withdrawals must wait 2 days (172,800 seconds) from request time.${NC}"
                continue
            fi
            
            read -p "Enter withdrawal ID to claim: " WITHDRAWAL_ID
            
            echo -e "${BLUE}Claiming withdrawal...${NC}"
            TX=$(zigchaind tx wasm execute $LP_POOL_ADDRESS \
                "{\"claim_withdraw\":{\"withdrawal_id\":$WITHDRAWAL_ID}}" \
                --from $WALLET \
                --node $NODE \
                --chain-id $CHAIN_ID \
                --gas-prices $GAS_PRICES \
                $GAS_AUTO \
                --output json \
                -y)
            
            echo "$TX" | jq '.'
            TXHASH=$(echo "$TX" | jq -r '.txhash')
            echo -e "${GREEN}Transaction hash:${NC} $TXHASH"
            echo -e "${YELLOW}Waiting for confirmation...${NC}"
            sleep 6
            
            RESULT=$(zigchaind query tx $TXHASH --node $NODE --output json 2>/dev/null)
            CODE=$(echo "$RESULT" | jq -r '.code')
            
            if [ "$CODE" = "0" ]; then
                echo -e "${GREEN}✓ Withdrawal claimed successfully!${NC}"
                echo -e "${BLUE}Check your stablecoin balance (option 8)${NC}"
            else
                echo -e "${YELLOW}Transaction result:${NC}"
                echo "$RESULT" | jq '.code, .raw_log'
            fi
            ;;
            
        4)
            echo -e "${YELLOW}Querying config...${NC}"
            CONFIG=$(zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
                '{"config":{}}' \
                --node $NODE \
                --output json | jq '.data')
            
            echo "$CONFIG"
            
            # Show withdrawal delay in human readable format
            DELAY=$(echo "$CONFIG" | jq -r '.withdrawal_delay')
            DAYS=$((DELAY / 86400))
            echo -e "${BLUE}Withdrawal delay: $DELAY seconds ($DAYS days)${NC}"
            ;;
            
        5)
            echo -e "${YELLOW}Querying vault info...${NC}"
            zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
                '{"vault_info":{}}' \
                --node $NODE \
                --output json | jq '.data'
            ;;
            
        6)
            echo -e "${YELLOW}Querying user info...${NC}"
            zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
                "{\"user_info\":{\"address\":\"$MY_ADDR\"}}" \
                --node $NODE \
                --output json | jq '.data'
            ;;
            
        7)
            echo -e "${YELLOW}Querying pending withdrawals...${NC}"
            PENDING=$(zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
                "{\"pending_withdrawals\":{\"address\":\"$MY_ADDR\"}}" \
                --node $NODE \
                --output json)
            
            echo "$PENDING" | jq '.data'
            
            # Show human-readable times
            CURRENT_TIME=$(date +%s)
            echo ""
            echo -e "${BLUE}Current time:${NC} $(date)"
            echo -e "${BLUE}Current timestamp:${NC} $CURRENT_TIME"
            
            # Check each withdrawal
            WITHDRAWALS=$(echo "$PENDING" | jq -r '.data.withdrawals[] | @json')
            if [ -n "$WITHDRAWALS" ]; then
                echo ""
                echo -e "${BLUE}Withdrawal Status:${NC}"
                while IFS= read -r withdrawal; do
                    WID=$(echo "$withdrawal" | jq -r '.id')
                    AMOUNT=$(echo "$withdrawal" | jq -r '.amount')
                    RELEASE=$(echo "$withdrawal" | jq -r '.release_time')
                    CLAIMABLE=$(echo "$withdrawal" | jq -r '.claimable')
                    
                    # Convert timestamp (nanoseconds to seconds)
                    RELEASE_SEC=$((RELEASE / 1000000000))
                    TIME_LEFT=$((RELEASE_SEC - CURRENT_TIME))
                    
                    echo -e "  ID: $WID | Amount: $AMOUNT"
                    if [ "$CLAIMABLE" = "true" ]; then
                        echo -e "  ${GREEN}✓ Ready to claim!${NC}"
                    else
                        HOURS_LEFT=$((TIME_LEFT / 3600))
                        MINS_LEFT=$(((TIME_LEFT % 3600) / 60))
                        echo -e "  ${YELLOW}⏳ Locked for $HOURS_LEFT hours, $MINS_LEFT minutes${NC}"
                    fi
                    echo ""
                done <<< "$WITHDRAWALS"
            fi
            ;;
            
        8)
            echo -e "${YELLOW}Checking balances...${NC}"
            echo -e "${BLUE}Stablecoin balance:${NC}"
            zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
                jq ".balances[] | select(.denom==\"$STABLECOIN_DENOM\")"
            
            echo -e "${BLUE}LP token balance:${NC}"
            zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
                jq ".balances[] | select(.denom==\"$LP_FULL_DENOM\")"
            ;;
        
        10)
            echo -e "${YELLOW}Admin: Withdraw Funds${NC}"
            echo -e "${BLUE}Withdraw stablecoin from vault to admin wallet${NC}"
            echo -e "${YELLOW}Note: Only admin can execute this${NC}"
            
            read -p "Enter amount to withdraw: " WITHDRAW_AMOUNT
            
            echo -e "${BLUE}Withdrawing funds to admin wallet...${NC}"
            TX=$(zigchaind tx wasm execute $LP_POOL_ADDRESS \
                "{\"admin_withdraw\":{\"amount\":\"$WITHDRAW_AMOUNT\"}}" \
                --from $WALLET \
                --node $NODE \
                --chain-id $CHAIN_ID \
                --gas-prices $GAS_PRICES \
                $GAS_AUTO \
                --output json \
                 -y)
            
            TXHASH=$(echo "$TX" | jq -r '.txhash')
            echo -e "${GREEN}Transaction hash:${NC} $TXHASH"
            sleep 6
            
            RESULT=$(zigchaind query tx $TXHASH --node $NODE --output json 2>/dev/null)
            CODE=$(echo "$RESULT" | jq -r '.code')
            
            if [ "$CODE" = "0" ]; then
                echo -e "${GREEN}✓ Funds withdrawn successfully!${NC}"
                echo -e "${BLUE}Funds are now in admin wallet for external management${NC}"
                echo -e "${YELLOW}Remember to deposit funds + yield back via option 11${NC}"
            else
                echo -e "${YELLOW}Transaction failed:${NC}"
                echo "$RESULT" | jq '.code, .raw_log'
            fi
            ;;
        
        11)
            echo -e "${YELLOW}Admin: Deposit Yield${NC}"
            echo -e "${BLUE}Deposit yield earned from external protocols${NC}"
            echo -e "${YELLOW}Note: Only admin can execute this${NC}"
            
            # Show current vault value
            echo -e "${BLUE}Current vault state:${NC}"
            zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
                '{"vault_info":{}}' \
                --node $NODE \
                --output json | jq '.data'
            
            echo ""
            echo -e "${BLUE}You need to specify:${NC}"
            echo -e "  ${GREEN}principal_amount${NC}: Previously withdrawn funds being returned (not added to TVL)"
            echo -e "  ${GREEN}yield_amount${NC}: New yield earned externally (added to TVL)"
            echo -e "  ${YELLOW}Total sent must equal principal_amount + yield_amount${NC}"
            echo ""
            
            read -p "Enter principal amount (or 0 if none): " PRINCIPAL_AMOUNT
            read -p "Enter yield amount: " YIELD_AMOUNT
            
            # Calculate total
            TOTAL_AMOUNT=$((PRINCIPAL_AMOUNT + YIELD_AMOUNT))
            
            echo -e "${BLUE}Summary:${NC}"
            echo -e "  Principal (returning): ${PRINCIPAL_AMOUNT}"
            echo -e "  Yield (new): ${YIELD_AMOUNT}"
            echo -e "  Total sending: ${TOTAL_AMOUNT}"
            echo ""
            read -p "Confirm? (y/n): " CONFIRM
            
            if [ "$CONFIRM" != "y" ]; then
                echo "Cancelled."
                continue
            fi
            
            echo -e "${BLUE}Depositing to vault...${NC}"
            TX=$(zigchaind tx wasm execute $LP_POOL_ADDRESS \
                "{\"admin_deposit_yield\":{\"principal_amount\":\"$PRINCIPAL_AMOUNT\",\"yield_amount\":\"$YIELD_AMOUNT\"}}" \
                --from $WALLET \
                --amount "${TOTAL_AMOUNT}${STABLECOIN_DENOM}" \
                --node $NODE \
                --chain-id $CHAIN_ID \
                --gas-prices $GAS_PRICES \
                $GAS_AUTO \
                --output json \
                -y)
            
            TXHASH=$(echo "$TX" | jq -r '.txhash')
            echo -e "${GREEN}Transaction hash:${NC} $TXHASH"
            sleep 6
            
            RESULT=$(zigchaind query tx $TXHASH --node $NODE --output json 2>/dev/null)
            CODE=$(echo "$RESULT" | jq -r '.code')
            
            if [ "$CODE" = "0" ]; then
                echo -e "${GREEN}✓ Yield deposited successfully!${NC}"
                
                # Extract info from events
                PRINCIPAL_RETURNED=$(echo "$RESULT" | jq -r '.events[] | select(.type=="wasm") | .attributes[] | select(.key=="principal_returned") | .value' | head -1)
                YIELD_DEPOSITED=$(echo "$RESULT" | jq -r '.events[] | select(.type=="wasm") | .attributes[] | select(.key=="yield_deposited") | .value' | head -1)
                TOTAL_RECEIVED=$(echo "$RESULT" | jq -r '.events[] | select(.type=="wasm") | .attributes[] | select(.key=="total_received") | .value' | head -1)
                NEW_TOTAL=$(echo "$RESULT" | jq -r '.events[] | select(.type=="wasm") | .attributes[] | select(.key=="new_total") | .value' | head -1)
                NEW_PRICE=$(echo "$RESULT" | jq -r '.events[] | select(.type=="wasm") | .attributes[] | select(.key=="price_per_share") | .value' | head -1)
                
                echo -e "${BLUE}Principal returned:${NC} $PRINCIPAL_RETURNED"
                echo -e "${BLUE}Yield deposited:${NC} $YIELD_DEPOSITED"
                echo -e "${BLUE}Total received:${NC} $TOTAL_RECEIVED"
                echo -e "${BLUE}New total value:${NC} $NEW_TOTAL"
                echo -e "${BLUE}New price per share:${NC} $NEW_PRICE"
                echo -e "${GREEN}All LP token holders benefit from the yield!${NC}"
            else
                echo -e "${YELLOW}Transaction failed:${NC}"
                echo "$RESULT" | jq '.code, .raw_log'
            fi
            ;;
        
        12)
            echo -e "${YELLOW}Admin: Update Config${NC}"
            echo -e "${YELLOW}Note: Only admin can execute this${NC}"
            echo -e "${BLUE}Leave blank to keep current value${NC}"
            
            read -p "New admin address (or blank): " NEW_ADMIN
            read -p "New stablecoin denom (or blank): " NEW_DENOM
            
            # Build JSON based on inputs
            CONFIG_JSON="{"
            FIRST=true
            
            if [ -n "$NEW_ADMIN" ]; then
                CONFIG_JSON+="\"admin\":\"$NEW_ADMIN\""
                FIRST=false
            fi
            
            if [ -n "$NEW_DENOM" ]; then
                if [ "$FIRST" = false ]; then
                    CONFIG_JSON+=","
                fi
                CONFIG_JSON+="\"stablecoin_denom\":\"$NEW_DENOM\""
            fi
            
            CONFIG_JSON+="}"
            
            if [ "$CONFIG_JSON" = "{}" ]; then
                echo -e "${YELLOW}No changes specified${NC}"
                continue
            fi
            
            echo -e "${BLUE}Updating config: $CONFIG_JSON${NC}"
            TX=$(zigchaind tx wasm execute $LP_POOL_ADDRESS \
                "{\"update_config\":$CONFIG_JSON}" \
                --from $WALLET \
                --node $NODE \
                --chain-id $CHAIN_ID \
                --gas-prices $GAS_PRICES \
                $GAS_AUTO \
                --output json \
                -y)
            
            TXHASH=$(echo "$TX" | jq -r '.txhash')
            echo -e "${GREEN}Transaction hash:${NC} $TXHASH"
            sleep 6
            
            RESULT=$(zigchaind query tx $TXHASH --node $NODE --output json 2>/dev/null)
            CODE=$(echo "$RESULT" | jq -r '.code')
            
            if [ "$CODE" = "0" ]; then
                echo -e "${GREEN}✓ Config updated!${NC}"
                echo -e "${BLUE}Query config (option 4) to verify changes${NC}"
            else
                echo -e "${YELLOW}Transaction failed:${NC}"
                echo "$RESULT" | jq '.code, .raw_log'
            fi
            ;;
            
        9)
            echo "Exiting..."
            exit 0
            ;;
            
        *)
            echo "Invalid choice. Please try again."
            ;;
    esac
    
    echo ""
    echo "Press Enter to continue..."
    read
    echo ""
done
