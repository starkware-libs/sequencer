#!/bin/bash

# Simple FuturesOrdered batching test
# Syncs 5000 blocks and compares WITH vs WITHOUT batching

set -e

BLOCKS_TO_SYNC=5000
BATCH_SIZE=100

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m'

# Cleanup function
cleanup() {
    echo ""
    echo -e "${YELLOW}Cleaning up...${NC}"
    
    # Kill any running nodes
    pkill -f apollo_node 2>/dev/null || true
    
    # Ask about data cleanup
    echo ""
    echo -e "${YELLOW}Test data directories:${NC}"
    du -sh ./data_with_batching ./data_without_batching 2>/dev/null || true
    
    read -p "Delete test data directories? (y/N): " -n 1 -r
    echo ""
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        rm -rf ./data_with_batching ./data_without_batching
        echo -e "${GREEN}✓ Test data cleaned up${NC}"
    else
        echo -e "${YELLOW}Test data preserved. Clean up manually with:${NC}"
        echo "  rm -rf ./data_with_batching ./data_without_batching"
    fi
}

# Set trap to cleanup on exit or interrupt
trap cleanup EXIT INT TERM

# Function to check disk space
check_disk_space() {
    local required_gb=$1
    local available_kb=$(df . | tail -1 | awk '{print $4}')
    local available_gb=$((available_kb / 1024 / 1024))
    
    echo -e "${BLUE}Disk space check:${NC}"
    echo "  Available: ${available_gb}GB"
    echo "  Required: ~${required_gb}GB"
    
    if [ $available_gb -lt $required_gb ]; then
        echo -e "${RED}ERROR: Not enough disk space!${NC}"
        echo -e "${RED}Need at least ${required_gb}GB free, but only ${available_gb}GB available${NC}"
        echo ""
        echo "Free up space or use a different location"
        exit 1
    fi
    
    echo -e "${GREEN}✓ Sufficient disk space${NC}"
}

# Monitor disk space during test
monitor_disk_space() {
    while true; do
        sleep 60
        local available_kb=$(df . | tail -1 | awk '{print $4}')
        local available_gb=$((available_kb / 1024 / 1024))
        
        if [ $available_gb -lt 5 ]; then
            echo -e "${RED}WARNING: Low disk space! Only ${available_gb}GB remaining${NC}"
            echo -e "${RED}Stopping test to prevent disk fill${NC}"
            kill $NODE_PID 2>/dev/null || true
            break
        fi
    done
}

# Function to show disk usage
show_disk_usage() {
    echo ""
    echo -e "${BLUE}Current disk usage:${NC}"
    df -h . | tail -1
    echo ""
    if [ -d "./data_with_batching" ]; then
        echo "  data_with_batching: $(du -sh ./data_with_batching 2>/dev/null | cut -f1)"
    fi
    if [ -d "./data_without_batching" ]; then
        echo "  data_without_batching: $(du -sh ./data_without_batching 2>/dev/null | cut -f1)"
    fi
    echo ""
}

echo "========================================"
echo "FUTURESORDERED BATCHING TEST"
echo "========================================"
echo ""

# Check disk space (need ~30GB for test: 2 x ~10GB databases + logs)
check_disk_space 30
echo ""

# Check for SN_MAIN database
if [ -d "./data_compare_batching/SN_MAIN" ]; then
    echo -e "${GREEN}✓ Found SN_MAIN database at: ./data_compare_batching/SN_MAIN${NC}"
    DB_SIZE=$(du -sh ./data_compare_batching/SN_MAIN 2>/dev/null | cut -f1)
    echo "  Size: $DB_SIZE"
    echo ""
    echo "The test will:"
    echo "  1. Link SN_MAIN to: ./data_with_batching/ (instant - no copying!)"
    echo "  2. Link SN_MAIN to: ./data_without_batching/ (instant - no copying!)"
    echo "  3. Run WITH batching - sync $BLOCKS_TO_SYNC more blocks"
    echo "  4. Run WITHOUT batching - sync $BLOCKS_TO_SYNC more blocks"
    echo "  5. Compare performance"
    echo ""
    echo -e "${GREEN}Uses hard links - instant setup, no disk space wasted${NC}"
    echo -e "${BLUE}Both tests start from the SAME block (whatever SN_MAIN is at)${NC}"
    echo -e "${YELLOW}Original SN_MAIN will NOT be modified${NC}"
else
    echo -e "${YELLOW}⚠ SN_MAIN not found at ./data_compare_batching/SN_MAIN${NC}"
    echo ""
    echo "The test will sync $BLOCKS_TO_SYNC blocks from block 0:"
    echo "  1. WITH FuturesOrdered batching"
    echo "  2. WITHOUT batching (sequential)"
    echo ""
    echo "Tip: For faster testing, sync some blocks first to create SN_MAIN"
fi
echo ""

# Build (skip if binary already exists)
if command -v apollo_node &> /dev/null; then
    echo -e "${GREEN}✓ Using pre-built apollo_node binary${NC}"
    echo "  Location: $(which apollo_node)"
elif [ -f "./target/release/apollo_node" ]; then
    echo -e "${GREEN}✓ Using existing apollo_node binary at ./target/release/apollo_node${NC}"
else
    echo "Building apollo_node..."
    cargo build --bin apollo_node --release
    echo -e "${GREEN}✓ Build complete${NC}"
fi
echo ""

# Test function
run_test() {
    local name=$1
    local enable_batching=$2
    local data_dir=$3
    
    echo ""
    echo "========================================"
    echo -e "${YELLOW}$name${NC}"
    echo "========================================"
    
    # Prepare data directory - use hard links from SN_MAIN (instant, no copying!)
    rm -rf "$data_dir"
    mkdir -p "$data_dir"
    
    if [ -d "./data_compare_batching/SN_MAIN" ]; then
        echo "  Creating database from SN_MAIN (using hard links - instant!)..."
        cp -al ./data_compare_batching/SN_MAIN/* "$data_dir/"
        echo -e "${GREEN}  ✓ Database created (no copying - hard links used)${NC}"
        echo -e "${BLUE}  (Node will resume from last synced block in SN_MAIN)${NC}"
        echo -e "${YELLOW}  (New data written to $data_dir only - SN_MAIN untouched)${NC}"
    else
        echo -e "${YELLOW}  No SN_MAIN found - starting from block 0${NC}"
    fi
    
    # Create minimal config override
    cat > test_config.json << EOF
{
  "state_sync_config.central_sync_client_config.sync_config.enable_block_batching": $enable_batching,
  "state_sync_config.central_sync_client_config.sync_config.block_batch_size": $BATCH_SIZE,
  "state_sync_config.storage_config.db_config.path_prefix": "$data_dir"
}
EOF
    
    # Run node
    export RUST_LOG=info
    LOG="test_$(echo $name | tr ' ' '_').log"
    
    echo "Starting node..."
    echo "  Data: $data_dir"
    echo "  Log: $LOG"
    
    # Determine config path (local vs K8s)
    if [ -d "/configs" ]; then
        # Running in K8s - use mounted configs
        CONFIG_PATH="/configs"
    else
        # Running locally - use repo structure
        CONFIG_PATH="crates/apollo_deployments/resources"
    fi
    
    # Use same config as run_mainnet_node.sh
    # Run with apollo_node if available, otherwise cargo run
    if command -v apollo_node &> /dev/null; then
        # Running with pre-built binary (K8s or local with binary)
        # In K8s, use all available config files; locally use full config
        if [ -d "/configs" ]; then
            # K8s: load ALL configs (they're all in /configs)
            apollo_node \
                --config_file $CONFIG_PATH/base_layer_config.json \
                --config_file $CONFIG_PATH/batcher_config.json \
                --config_file $CONFIG_PATH/class_manager_config.json \
                --config_file $CONFIG_PATH/consensus_manager_config.json \
                --config_file $CONFIG_PATH/revert_config.json \
                --config_file $CONFIG_PATH/versioned_constants_overrides_config.json \
                --config_file $CONFIG_PATH/validate_resource_bounds_config.json \
                --config_file $CONFIG_PATH/gateway_config.json \
                --config_file $CONFIG_PATH/http_server_config.json \
                --config_file $CONFIG_PATH/l1_endpoint_monitor_config.json \
                --config_file $CONFIG_PATH/l1_gas_price_provider_config.json \
                --config_file $CONFIG_PATH/l1_gas_price_scraper_config.json \
                --config_file $CONFIG_PATH/l1_provider_config.json \
                --config_file $CONFIG_PATH/l1_scraper_config.json \
                --config_file $CONFIG_PATH/mempool_config.json \
                --config_file $CONFIG_PATH/mempool_p2p_config.json \
                --config_file $CONFIG_PATH/monitoring_endpoint_config.json \
                --config_file $CONFIG_PATH/sierra_compiler_config.json \
                --config_file $CONFIG_PATH/state_sync_config.json \
                --config_file $CONFIG_PATH/mainnet_deployment \
                --config_file $CONFIG_PATH/mainnet_hybrid \
                --config_file $CONFIG_PATH/node_config \
                --config_file $CONFIG_PATH/minimal_node_config.json \
                --config_file $CONFIG_PATH/mainnet_secrets.json \
                --config_file test_config.json \
                > "$LOG" 2>&1 &
        else
            # Local with binary: use full config
            apollo_node \
                --config_file $CONFIG_PATH/app_configs/base_layer_config.json \
                --config_file $CONFIG_PATH/app_configs/batcher_config.json \
                --config_file $CONFIG_PATH/app_configs/class_manager_config.json \
                --config_file $CONFIG_PATH/app_configs/consensus_manager_config.json \
                --config_file $CONFIG_PATH/app_configs/revert_config.json \
                --config_file $CONFIG_PATH/app_configs/versioned_constants_overrides_config.json \
                --config_file $CONFIG_PATH/app_configs/validate_resource_bounds_config.json \
                --config_file $CONFIG_PATH/app_configs/gateway_config.json \
                --config_file $CONFIG_PATH/app_configs/http_server_config.json \
                --config_file $CONFIG_PATH/app_configs/l1_endpoint_monitor_config.json \
                --config_file $CONFIG_PATH/app_configs/l1_gas_price_provider_config.json \
                --config_file $CONFIG_PATH/app_configs/l1_gas_price_scraper_config.json \
                --config_file $CONFIG_PATH/app_configs/l1_provider_config.json \
                --config_file $CONFIG_PATH/app_configs/l1_scraper_config.json \
                --config_file $CONFIG_PATH/app_configs/mempool_config.json \
                --config_file $CONFIG_PATH/app_configs/mempool_p2p_config.json \
                --config_file $CONFIG_PATH/app_configs/monitoring_endpoint_config.json \
                --config_file $CONFIG_PATH/app_configs/sierra_compiler_config.json \
                --config_file $CONFIG_PATH/app_configs/state_sync_config.json \
                --config_file $CONFIG_PATH/deployments/mainnet/deployment_config_override.json \
                --config_file $CONFIG_PATH/deployments/mainnet/hybrid_0.json \
                --config_file $CONFIG_PATH/services/consolidated/node.json \
                --config_file $CONFIG_PATH/mainnet_secrets.json \
                --config_file test_config.json \
                > "$LOG" 2>&1 &
        fi
    else
        cargo run --release --bin apollo_node -- \
            --config_file $CONFIG_PATH/app_configs/base_layer_config.json \
            --config_file $CONFIG_PATH/app_configs/batcher_config.json \
            --config_file $CONFIG_PATH/app_configs/class_manager_config.json \
            --config_file $CONFIG_PATH/app_configs/consensus_manager_config.json \
            --config_file $CONFIG_PATH/app_configs/revert_config.json \
            --config_file $CONFIG_PATH/app_configs/versioned_constants_overrides_config.json \
            --config_file $CONFIG_PATH/app_configs/validate_resource_bounds_config.json \
            --config_file $CONFIG_PATH/app_configs/gateway_config.json \
            --config_file $CONFIG_PATH/app_configs/http_server_config.json \
            --config_file $CONFIG_PATH/app_configs/l1_endpoint_monitor_config.json \
            --config_file $CONFIG_PATH/app_configs/l1_gas_price_provider_config.json \
            --config_file $CONFIG_PATH/app_configs/l1_gas_price_scraper_config.json \
            --config_file $CONFIG_PATH/app_configs/l1_provider_config.json \
            --config_file $CONFIG_PATH/app_configs/l1_scraper_config.json \
            --config_file $CONFIG_PATH/app_configs/mempool_config.json \
            --config_file $CONFIG_PATH/app_configs/mempool_p2p_config.json \
            --config_file $CONFIG_PATH/app_configs/monitoring_endpoint_config.json \
            --config_file $CONFIG_PATH/app_configs/sierra_compiler_config.json \
            --config_file $CONFIG_PATH/app_configs/state_sync_config.json \
            --config_file $CONFIG_PATH/deployments/mainnet/deployment_config_override.json \
            --config_file $CONFIG_PATH/deployments/mainnet/hybrid_0.json \
            --config_file $CONFIG_PATH/services/consolidated/node.json \
            --config_file $CONFIG_PATH/mainnet_secrets.json \
            --config_file test_config.json \
            > "$LOG" 2>&1 &
    fi
    
    PID=$!
    NODE_PID=$PID
    echo "  PID: $PID"
    sleep 3
    
    # Start disk space monitor in background
    monitor_disk_space &
    MONITOR_PID=$!
    
    # Monitor
    START=$(date +%s)
    LAST=0
    DISK_CHECK_COUNTER=0
    
    while kill -0 $PID 2>/dev/null; do
        sleep 2
        
        # Different detection for batching vs non-batching
        if [ "$enable_batching" = "true" ]; then
            # Count batch completions and multiply by batch size
            COUNT=$(grep -c "Successfully wrote.*blocks to storage" "$LOG" 2>/dev/null || echo 0)
            COUNT=$(echo "$COUNT" | tr -d '\n\r ' | head -1)
            [ -z "$COUNT" ] && COUNT=0
            DONE=$((COUNT * BATCH_SIZE))
        else
            # Count individual block completions
            DONE=$(grep -c "SYNC_NEW_BLOCK: Added block" "$LOG" 2>/dev/null || echo 0)
            DONE=$(echo "$DONE" | tr -d '\n\r ' | head -1)
            [ -z "$DONE" ] && DONE=0
        fi
        
        if [ "$DONE" -gt "$LAST" ] 2>/dev/null; then
            echo -e "${GREEN}  Progress: $DONE/$BLOCKS_TO_SYNC blocks${NC}"
            LAST=$DONE
            
            # Show disk usage every 10 progress updates
            DISK_CHECK_COUNTER=$((DISK_CHECK_COUNTER + 1))
            if [ $((DISK_CHECK_COUNTER % 10)) -eq 0 ]; then
                local available_kb=$(df . | tail -1 | awk '{print $4}')
                local available_gb=$((available_kb / 1024 / 1024))
                echo -e "${BLUE}  Disk: ${available_gb}GB free, Data dir: $(du -sh $data_dir 2>/dev/null | cut -f1)${NC}"
            fi
            
            if [ "$DONE" -ge "$BLOCKS_TO_SYNC" ]; then
                echo -e "${GREEN}  ✓ Done! Synced $DONE blocks${NC}"
                kill $PID 2>/dev/null || true
                kill $MONITOR_PID 2>/dev/null || true
                break
            fi
        fi
    done
    
    # Stop disk monitor
    kill $MONITOR_PID 2>/dev/null || true
    wait $PID 2>/dev/null || true
    END=$(date +%s)
    DURATION=$((END - START))
    
    echo "$DURATION" > "time_$name.txt"
    echo -e "${GREEN}  Completed in: ${DURATION}s${NC}"
    
    # If node completed too quickly (< 5s), show the error log
    if [ "$DURATION" -lt 5 ]; then
        echo ""
        echo -e "${RED}⚠ Node completed very quickly - showing log:${NC}"
        echo "---"
        tail -100 "$LOG" || echo "Log file not readable"
        echo "---"
    fi
}

# Run tests
run_test "WITH_BATCHING" "true" "./data_with_batching"
run_test "WITHOUT_BATCHING" "false" "./data_without_batching"

# Results
echo ""
echo "========================================"
echo "RESULTS"
echo "========================================"

WITH=$(cat "time_WITH_BATCHING.txt")
WITHOUT=$(cat "time_WITHOUT_BATCHING.txt")

echo ""
echo "Time to sync $BLOCKS_TO_SYNC blocks:"
echo "  WITH batching:    ${WITH}s"
echo "  WITHOUT batching: ${WITHOUT}s"
echo ""

if [ "$WITH" -lt "$WITHOUT" ]; then
    SPEEDUP=$(awk "BEGIN {printf \"%.2f\", $WITHOUT / $WITH}")
    PCT=$(awk "BEGIN {printf \"%.1f\", 100 * ($WITHOUT - $WITH) / $WITHOUT}")
    echo -e "${GREEN}✓ BATCHING IS ${SPEEDUP}x FASTER ($PCT% improvement)${NC}"
else
    echo -e "${RED}✗ Batching is slower${NC}"
fi

echo ""
echo "Logs: test_WITH_BATCHING.log, test_WITHOUT_BATCHING.log"
echo ""

# Show final disk usage
show_disk_usage

echo "========================================"
echo "DONE"
echo "========================================"

# Cleanup
rm -f test_config.json

