#!/bin/bash

# ZigChain Token Vault - TokenFactory Edition Interaction Script

set -e

# Configuration
NODE="https://public-zigchain-testnet-rpc.numia.xyz:443"
CHAIN_ID="zig-test-2"
WALLET="mynewwallet"
GAS_PRICES="0.025uzig"
GAS_AUTO="--gas auto --gas-adjustment 1.5"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Load contract addresses
if [ -f "scripts/vault_addresses.txt" ]; then
    source scripts/vault_addresses.txt
else
    echo "Error: scripts/vault_addresses.txt not found. Run deploy_tokenfactory.sh first."
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
    echo "1) Deposit (send stablecoin, receive LP tokens)"
    echo "2) Withdraw (send LP tokens, receive stablecoin)"
    echo "3) Query config"
    echo "4) Query vault info"
    echo "5) Query user info"
    echo "6) Check balances"
    echo "7) Exit"
    echo ""
    read -p "Enter choice [1-7]: " choice

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
            
            echo -e "${YELLOW}Transaction result:${NC}"
            zigchaind query tx $TXHASH --node $NODE --output json | jq '.code, .raw_log'
            ;;
            
        2)
            echo -e "${YELLOW}Withdraw${NC}"
            read -p "Enter amount of LP tokens to withdraw: " LP_AMOUNT
            
            echo -e "${BLUE}Sending withdrawal transaction...${NC}"
            TX=$(zigchaind tx wasm execute $LP_POOL_ADDRESS \
                '{"withdraw":{}}' \
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
            
            echo -e "${YELLOW}Transaction result:${NC}"
            zigchaind query tx $TXHASH --node $NODE --output json | jq '.code, .raw_log'
            ;;
            
        3)
            echo -e "${YELLOW}Querying config...${NC}"
            zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
                '{"config":{}}' \
                --node $NODE \
                --output json | jq '.data'
            ;;
            
        4)
            echo -e "${YELLOW}Querying vault info...${NC}"
            zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
                '{"vault_info":{}}' \
                --node $NODE \
                --output json | jq '.data'
            ;;
            
        5)
            echo -e "${YELLOW}Querying user info...${NC}"
            zigchaind query wasm contract-state smart $LP_POOL_ADDRESS \
                "{\"user_info\":{\"address\":\"$MY_ADDR\"}}" \
                --node $NODE \
                --output json | jq '.data'
            ;;
            
        6)
            echo -e "${YELLOW}Checking balances...${NC}"
            echo -e "${BLUE}Stablecoin balance:${NC}"
            zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
                jq ".balances[] | select(.denom==\"$STABLECOIN_DENOM\")"
            
            echo -e "${BLUE}LP token balance:${NC}"
            zigchaind query bank balances $MY_ADDR --node $NODE --output json | \
                jq ".balances[] | select(.denom==\"$LP_FULL_DENOM\")"
            ;;
            
        7)
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
