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

echo "========================================"
echo "FUTURESORDERED BATCHING TEST"
echo "========================================"
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

# Build
echo "Building..."
cargo build --bin apollo_node --release
echo -e "${GREEN}✓ Build complete${NC}"
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
    
    # Use same config as run_mainnet_node.sh
    cargo run --release --bin apollo_node -- \
        --config_file crates/apollo_deployments/resources/app_configs/base_layer_config.json \
        --config_file crates/apollo_deployments/resources/app_configs/batcher_config.json \
        --config_file crates/apollo_deployments/resources/app_configs/class_manager_config.json \
        --config_file crates/apollo_deployments/resources/app_configs/consensus_manager_config.json \
        --config_file crates/apollo_deployments/resources/app_configs/revert_config.json \
        --config_file crates/apollo_deployments/resources/app_configs/versioned_constants_overrides_config.json \
        --config_file crates/apollo_deployments/resources/app_configs/validate_resource_bounds_config.json \
        --config_file crates/apollo_deployments/resources/app_configs/gateway_config.json \
        --config_file crates/apollo_deployments/resources/app_configs/http_server_config.json \
        --config_file crates/apollo_deployments/resources/app_configs/l1_endpoint_monitor_config.json \
        --config_file crates/apollo_deployments/resources/app_configs/l1_gas_price_provider_config.json \
        --config_file crates/apollo_deployments/resources/app_configs/l1_gas_price_scraper_config.json \
        --config_file crates/apollo_deployments/resources/app_configs/l1_provider_config.json \
        --config_file crates/apollo_deployments/resources/app_configs/l1_scraper_config.json \
        --config_file crates/apollo_deployments/resources/app_configs/mempool_config.json \
        --config_file crates/apollo_deployments/resources/app_configs/mempool_p2p_config.json \
        --config_file crates/apollo_deployments/resources/app_configs/monitoring_endpoint_config.json \
        --config_file crates/apollo_deployments/resources/app_configs/sierra_compiler_config.json \
        --config_file crates/apollo_deployments/resources/app_configs/state_sync_config.json \
        --config_file crates/apollo_deployments/resources/deployments/mainnet/deployment_config_override.json \
        --config_file crates/apollo_deployments/resources/deployments/mainnet/hybrid_0.json \
        --config_file crates/apollo_deployments/resources/services/consolidated/node.json \
        --config_file crates/apollo_deployments/resources/mainnet_secrets.json \
        --config_file test_config.json \
        > "$LOG" 2>&1 &
    
    PID=$!
    echo "  PID: $PID"
    sleep 3
    
    # Monitor
    START=$(date +%s)
    LAST=0
    
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
            
            if [ "$DONE" -ge "$BLOCKS_TO_SYNC" ]; then
                echo -e "${GREEN}  ✓ Done! Synced $DONE blocks${NC}"
                kill $PID 2>/dev/null || true
                break
            fi
        fi
    done
    
    wait $PID 2>/dev/null || true
    END=$(date +%s)
    DURATION=$((END - START))
    
    echo "$DURATION" > "time_$name.txt"
    echo -e "${GREEN}  Completed in: ${DURATION}s${NC}"
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
echo "========================================"
echo "DONE"
echo "========================================"

# Cleanup
rm -f test_config.json

