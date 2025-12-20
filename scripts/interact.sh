#!/bin/bash

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

if [ -f "scripts/contract_addresses.txt" ]; then
    source scripts/contract_addresses.txt
elif [ -f "contract_addresses.txt" ]; then
    source contract_addresses.txt
else
    echo -e "${RED}Error: contract_addresses.txt not found!${NC}"
    echo "Run ./scripts/deploy.sh first"
    exit 1
fi

WALLET_NAME="mynewwallet"

# Helper function to query and display balance
check_balances() {
    echo -e "\n${BLUE}=== Current Balances ===${NC}"
    
    # USDT balance
    USDT_BAL=$(zigchaind query wasm contract-state smart $STABLECOIN_ADDRESS \
        "{\"balance\":{\"address\":\"$WALLET_ADDRESS\"}}" \
        --node $NODE -o json | jq -r '.data.balance')
    echo "USDT Balance: $(echo "scale=6; $USDT_BAL / 1000000" | bc) USDT"
    
    # LP token balance
    LP_BAL=$(zigchaind query wasm contract-state smart $LP_TOKEN_ADDRESS \
        "{\"balance\":{\"address\":\"$WALLET_ADDRESS\"}}" \
        --node $NODE -o json | jq -r '.data.balance')
    echo "LP Token Balance: $(echo "scale=6; $LP_BAL / 1000000" | bc) LP"
    
    # Pool info
    POOL_INFO=$(zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
        '{"pool_info":{}}' \
        --node $NODE -o json)
    TOTAL_DEPOSIT=$(echo $POOL_INFO | jq -r '.data.total_stablecoin_deposited')
    TOTAL_LP=$(echo $POOL_INFO | jq -r '.data.total_lp_supply')
    
    echo "Pool Total USDT: $(echo "scale=6; $TOTAL_DEPOSIT / 1000000" | bc) USDT"
    echo "Pool Total LP: $(echo "scale=6; $TOTAL_LP / 1000000" | bc) LP"
}

# Function to deposit
deposit() {
    local amount=$1
    local micro_amount=$((amount * 1000000))
    
    echo -e "\n${YELLOW}=== Depositing $amount USDT ===${NC}"
    
    # Step 1: Approve
    echo "Step 1: Approving LP pool contract..."
    zigchaind tx wasm execute $STABLECOIN_ADDRESS \
        "{\"increase_allowance\":{\"spender\":\"$LP_POOL_ADDRESS\",\"amount\":\"$micro_amount\"}}" \
        --from $WALLET_NAME \
        --node $NODE \
        --chain-id $CHAIN_ID \
        --gas 300000 \
        --fees 15000uzig \
        -y > /dev/null
    
    echo "Waiting for approval..."
    sleep 6
    echo -e "${GREEN}✓ Approved${NC}"
    
    # Step 2: Deposit
    echo "Step 2: Executing deposit..."
    DEPOSIT_TX=$(zigchaind tx wasm execute $LP_POOL_ADDRESS \
        "{\"deposit\":{\"amount\":\"$micro_amount\"}}" \
        --from $WALLET_NAME \
        --node $NODE \
        --chain-id $CHAIN_ID \
        --gas 500000 \
        --fees 25000uzig \
        -y --output json | jq -r '.txhash')
    
    echo "Waiting for deposit transaction..."
    sleep 6
    
    # Check transaction
    TX_RESULT=$(zigchaind query tx $DEPOSIT_TX --node $NODE -o json)
    if echo $TX_RESULT | jq -e '.code == 0' > /dev/null; then
        echo -e "${GREEN}✓ Deposit successful!${NC}"
        echo "Transaction: $DEPOSIT_TX"
        
        # Extract events
        echo -e "\n${BLUE}Events:${NC}"
        echo $TX_RESULT | jq -r '.events[] | select(.type=="wasm") | .attributes[] | "\(.key): \(.value)"' | grep -E "method|user|amount"
    else
        echo -e "${RED}✗ Deposit failed!${NC}"
        echo $TX_RESULT | jq -r '.raw_log'
    fi
}

# Function to withdraw
withdraw() {
    local amount=$1
    local micro_amount=$((amount * 1000000))
    
    echo -e "\n${YELLOW}=== Withdrawing $amount LP ===${NC}"
    
    # Step 1: Approve LP token burn
    echo "Step 1: Approving LP token burn..."
    zigchaind tx wasm execute $LP_TOKEN_ADDRESS \
        "{\"increase_allowance\":{\"spender\":\"$LP_POOL_ADDRESS\",\"amount\":\"$micro_amount\"}}" \
        --from $WALLET_NAME \
        --node $NODE \
        --chain-id $CHAIN_ID \
        --gas 300000 \
        --fees 15000uzig \
        -y > /dev/null
    
    echo "Waiting for approval..."
    sleep 6
    echo -e "${GREEN}✓ Approved${NC}"
    
    # Step 2: Withdraw
    echo "Step 2: Executing withdrawal..."
    WITHDRAW_TX=$(zigchaind tx wasm execute $LP_POOL_ADDRESS \
        "{\"withdraw\":{\"amount\":\"$micro_amount\"}}" \
        --from $WALLET_NAME \
        --node $NODE \
        --chain-id $CHAIN_ID \
        --gas 500000 \
        --fees 25000uzig \
        -y --output json | jq -r '.txhash')
    
    echo "Waiting for withdrawal transaction..."
    sleep 6
    
    # Check transaction
    TX_RESULT=$(zigchaind query tx $WITHDRAW_TX --node $NODE -o json)
    if echo $TX_RESULT | jq -e '.code == 0' > /dev/null; then
        echo -e "${GREEN}✓ Withdrawal successful!${NC}"
        echo "Transaction: $WITHDRAW_TX"
        
        # Extract events
        echo -e "\n${BLUE}Events:${NC}"
        echo $TX_RESULT | jq -r '.events[] | select(.type=="wasm") | .attributes[] | "\(.key): \(.value)"' | grep -E "method|user|amount"
    else
        echo -e "${RED}✗ Withdrawal failed!${NC}"
        echo $TX_RESULT | jq -r '.raw_log'
    fi
}

# Query user info
query_user_info() {
    echo -e "\n${BLUE}=== User Info ===${NC}"
    USER_INFO=$(zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
        "{\"user_info\":{\"address\":\"$WALLET_ADDRESS\"}}" \
        --node $NODE -o json | jq '.data')
    
    echo $USER_INFO | jq .
}

# Main menu
show_menu() {
    echo ""
    echo -e "${GREEN}========================================${NC}"
    echo -e "${GREEN}LP Pool Interaction Menu${NC}"
    echo -e "${GREEN}========================================${NC}"
    echo "1. Check Balances"
    echo "2. Deposit USDT"
    echo "3. Withdraw LP Tokens"
    echo "4. Query User Info"
    echo "5. Query Pool Config"
    echo "6. Run Full Test (Deposit + Withdraw)"
    echo "0. Exit"
    echo ""
}

# Query config
query_config() {
    echo -e "\n${BLUE}=== Pool Configuration ===${NC}"
    CONFIG=$(zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
        '{"config":{}}' \
        --node $NODE -o json | jq '.data')
    
    echo $CONFIG | jq .
}

# Full test
full_test() {
    echo -e "\n${GREEN}=== Running Full Test ===${NC}"
    echo "This will:"
    echo "  1. Check initial balances"
    echo "  2. Deposit 100 USDT"
    echo "  3. Check balances after deposit"
    echo "  4. Withdraw 50 LP"
    echo "  5. Check final balances"
    echo ""
    read -p "Continue? (y/n): " confirm
    
    if [ "$confirm" != "y" ]; then
        return
    fi
    
    check_balances
    deposit 100
    check_balances
    withdraw 50
    check_balances
    
    echo -e "\n${GREEN}✓ Full test completed!${NC}"
}

# Main loop
while true; do
    show_menu
    read -p "Enter choice: " choice
    
    case $choice in
        1)
            check_balances
            ;;
        2)
            read -p "Enter USDT amount to deposit: " amount
            deposit $amount
            ;;
        3)
            read -p "Enter LP amount to withdraw: " amount
            withdraw $amount
            ;;
        4)
            query_user_info
            ;;
        5)
            query_config
            ;;
        6)
            full_test
            ;;
        0)
            echo "Goodbye!"
            exit 0
            ;;
        *)
            echo -e "${RED}Invalid choice${NC}"
            ;;
    esac
done
