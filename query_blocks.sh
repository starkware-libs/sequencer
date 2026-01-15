#!/bin/bash

# Configuration
RPC_URL="http://localhost:8090/rpc"
NAMESPACE="starknet"  # Change this to your namespace
POD_NAME="sequencer-node-0"  # Change this to your pod name

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "=== Real-Time Block Query Tool ==="
echo ""

# Function to check if port-forward is running
check_port_forward() {
    if ! curl -s -X POST -H 'Content-Type: application/json' \
        -d '{"jsonrpc":"2.0","id":"1","method":"starknet_blockNumber","params":[]}' \
        $RPC_URL > /dev/null 2>&1; then
        echo -e "${RED}❌ Cannot connect to RPC!${NC}"
        echo ""
        echo "Make sure port-forward is running:"
        echo "  kubectl port-forward -n $NAMESPACE $POD_NAME 8090:8090"
        echo ""
        exit 1
    fi
}

# Function to get latest block
get_latest_block() {
    curl -s -X POST -H 'Content-Type: application/json' \
        -d '{"jsonrpc":"2.0","id":"1","method":"starknet_blockNumber","params":[]}' \
        $RPC_URL | jq -r '.result // "error"'
}

# Function to get block details (with transaction hashes)
get_block_with_tx_hashes() {
    local block_num=$1
    curl -s -X POST -H 'Content-Type: application/json' \
        -d "{\"jsonrpc\":\"2.0\",\"id\":\"1\",\"method\":\"starknet_getBlockWithTxHashes\",\"params\":[{\"block_number\":$block_num}]}" \
        $RPC_URL
}

# Function to get block with full transactions
get_block_with_txs() {
    local block_num=$1
    curl -s -X POST -H 'Content-Type: application/json' \
        -d "{\"jsonrpc\":\"2.0\",\"id\":\"1\",\"method\":\"starknet_getBlockWithTxs\",\"params\":[{\"block_number\":$block_num}]}" \
        $RPC_URL
}

# Function to get block with receipts (most complete)
get_block_with_receipts() {
    local block_num=$1
    curl -s -X POST -H 'Content-Type: application/json' \
        -d "{\"jsonrpc\":\"2.0\",\"id\":\"1\",\"method\":\"starknet_getBlockWithReceipts\",\"params\":[{\"block_number\":$block_num}]}" \
        $RPC_URL
}

# Function to get state update (state diff)
get_state_update() {
    local block_num=$1
    curl -s -X POST -H 'Content-Type: application/json' \
        -d "{\"jsonrpc\":\"2.0\",\"id\":\"1\",\"method\":\"starknet_getStateUpdate\",\"params\":[{\"block_number\":$block_num}]}" \
        $RPC_URL
}

# Check connection first
check_port_forward

echo -e "${GREEN}✅ Connected to RPC${NC}"
echo ""

# Main menu
while true; do
    echo "======================================"
    echo "What would you like to do?"
    echo "1) Get latest block number"
    echo "2) Get block with TX hashes"
    echo "3) Get block with full transactions"
    echo "4) Get block with receipts (most complete)"
    echo "5) Get state diff for block"
    echo "6) Query random block"
    echo "7) Monitor blocks (real-time)"
    echo "8) Verify storage (test random blocks)"
    echo "0) Exit"
    echo "======================================"
    read -p "Choose option: " option
    
    case $option in
        1)
            echo ""
            echo "Fetching latest block..."
            LATEST=$(get_latest_block)
            if [ "$LATEST" != "error" ]; then
                echo -e "${GREEN}Latest block: $LATEST${NC}"
            else
                echo -e "${RED}Failed to get latest block${NC}"
            fi
            echo ""
            ;;
        
        2)
            echo ""
            read -p "Enter block number: " BLOCK_NUM
            echo "Fetching block $BLOCK_NUM with TX hashes..."
            RESULT=$(get_block_with_tx_hashes $BLOCK_NUM)
            if echo "$RESULT" | jq -e '.result' > /dev/null 2>&1; then
                echo -e "${GREEN}Block $BLOCK_NUM:${NC}"
                echo "$RESULT" | jq '.result.header'
                echo "Transaction hashes:"
                echo "$RESULT" | jq '.result.transactions'
            else
                echo -e "${RED}Failed to fetch block $BLOCK_NUM${NC}"
                echo "$RESULT" | jq
            fi
            echo ""
            ;;
        
        3)
            echo ""
            read -p "Enter block number: " BLOCK_NUM
            echo "Fetching block $BLOCK_NUM with full transactions..."
            RESULT=$(get_block_with_txs $BLOCK_NUM)
            if echo "$RESULT" | jq -e '.result' > /dev/null 2>&1; then
                echo -e "${GREEN}Block $BLOCK_NUM with full transactions:${NC}"
                echo "$RESULT" | jq
            else
                echo -e "${RED}Failed to fetch block $BLOCK_NUM${NC}"
                echo "$RESULT" | jq
            fi
            echo ""
            ;;
        
        4)
            echo ""
            read -p "Enter block number: " BLOCK_NUM
            echo "Fetching block $BLOCK_NUM with receipts (most complete data)..."
            RESULT=$(get_block_with_receipts $BLOCK_NUM)
            if echo "$RESULT" | jq -e '.result' > /dev/null 2>&1; then
                echo -e "${GREEN}Block $BLOCK_NUM with receipts:${NC}"
                echo "$RESULT" | jq
            else
                echo -e "${RED}Failed to fetch block $BLOCK_NUM${NC}"
                echo "$RESULT" | jq
            fi
            echo ""
            ;;
        
        5)
            echo ""
            read -p "Enter block number: " BLOCK_NUM
            echo "Fetching state diff for block $BLOCK_NUM..."
            RESULT=$(get_state_update $BLOCK_NUM)
            if echo "$RESULT" | jq -e '.result' > /dev/null 2>&1; then
                echo -e "${GREEN}State update for block $BLOCK_NUM:${NC}"
                echo "$RESULT" | jq
            else
                echo -e "${RED}Failed to fetch state update for block $BLOCK_NUM${NC}"
                echo "$RESULT" | jq
            fi
            echo ""
            ;;
        
        6)
            echo ""
            LATEST=$(get_latest_block)
            if [ "$LATEST" != "error" ] && [ "$LATEST" -gt 0 ]; then
                RANDOM_BLOCK=$((RANDOM % LATEST))
                echo "Fetching random block: $RANDOM_BLOCK (out of $LATEST)..."
                RESULT=$(get_block_with_receipts $RANDOM_BLOCK)
                if echo "$RESULT" | jq -e '.result' > /dev/null 2>&1; then
                    echo -e "${GREEN}Block $RANDOM_BLOCK found:${NC}"
                    echo "$RESULT" | jq '.result.header'
                else
                    echo -e "${RED}Failed to fetch block $RANDOM_BLOCK${NC}"
                fi
            else
                echo -e "${RED}Could not determine latest block${NC}"
            fi
            echo ""
            ;;
        
        7)
            echo ""
            echo "Monitoring blocks (Ctrl+C to stop)..."
            LAST_BLOCK=0
            while true; do
                CURRENT=$(get_latest_block)
                if [ "$CURRENT" != "error" ] && [ "$CURRENT" != "$LAST_BLOCK" ]; then
                    echo -e "${YELLOW}$(date '+%H:%M:%S')${NC} - New block: ${GREEN}$CURRENT${NC}"
                    LAST_BLOCK=$CURRENT
                fi
                sleep 2
            done
            ;;
        
        8)
            echo ""
            read -p "How many random blocks to test? " COUNT
            echo "Testing $COUNT random blocks using getBlockWithReceipts..."
            LATEST=$(get_latest_block)
            SUCCESS=0
            FAILED=0
            
            for i in $(seq 1 $COUNT); do
                BLOCK=$((RANDOM % LATEST))
                printf "Testing block %-8d ... " $BLOCK
                RESULT=$(get_block_with_receipts $BLOCK)
                if echo "$RESULT" | jq -e '.result' > /dev/null 2>&1; then
                    echo -e "${GREEN}✅ OK${NC}"
                    ((SUCCESS++))
                else
                    echo -e "${RED}❌ FAILED${NC}"
                    ((FAILED++))
                fi
            done
            
            echo ""
            echo "===== Results ====="
            echo -e "${GREEN}Success: $SUCCESS${NC}"
            echo -e "${RED}Failed: $FAILED${NC}"
            echo ""
            ;;
        
        0)
            echo "Goodbye!"
            exit 0
            ;;
        
        *)
            echo -e "${RED}Invalid option${NC}"
            ;;
    esac
done

