#!/bin/bash
# Storage Batching Performance Test
# Syncs blocks WITH and WITHOUT batching and compares performance

set -e

# Configuration from environment variables
BLOCKS_TO_SYNC=${BLOCKS_TO_SYNC:-50000}
BATCH_SIZE=${BATCH_SIZE:-100}

echo "========================================"
echo "STORAGE BATCHING TEST"
echo "========================================"
echo "Blocks to sync: $BLOCKS_TO_SYNC"
echo "Batch size: $BATCH_SIZE"
echo ""

# Determine config path (K8s vs local)
if [ -d "/configs" ]; then
    CONFIG_PATH="/configs"
else
    CONFIG_PATH="crates/apollo_deployments/resources"
fi

# Test function
run_test() {
    local name=$1
    local batch_size=$2
    local data_dir=$3
    
    echo ""
    echo "========================================"
    echo "$name"
    echo "========================================"
    
    # Prepare data directory
    rm -rf "$data_dir"
    mkdir -p "$data_dir"
    
    # Create config override
    cat > test_config.json << EOF
{
  "state_sync_config.storage_config.batch_config.batch_size": $batch_size,
  "state_sync_config.storage_config.db_config.path_prefix": "$data_dir"
}
EOF
    
    # Run node
    export RUST_LOG=info
    LOG="test_$(echo $name | tr ' ' '_').log"
    
    echo "Starting node..."
    echo "  Data: $data_dir"
    echo "  Batch size: $batch_size"
    echo "  Log: $LOG"
    
    # Run with apollo_node if available, otherwise cargo run
    if command -v apollo_node &> /dev/null; then
        if [ -d "/configs" ]; then
            # K8s: use mounted configs
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
                --config_file $CONFIG_PATH/mainnet_secrets.json \
                --config_file test_config.json \
                > "$LOG" 2>&1 &
        else
            # Local: use repo structure
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
    echo "  PID: $PID"
    sleep 3
    
    # Monitor progress
    START=$(date +%s)
    LAST=0
    
    while kill -0 $PID 2>/dev/null; do
        sleep 2
        
        # Count blocks synced (different detection for batching vs non-batching)
        if [ "$batch_size" -gt 1 ]; then
            # With batching: count batch completions and multiply
            COUNT=$(grep -c "Successfully wrote.*blocks to storage" "$LOG" 2>/dev/null || echo 0)
            DONE=$((COUNT * batch_size))
        else
            # Without batching: count individual blocks
            DONE=$(grep -c "SYNC_NEW_BLOCK: Added block" "$LOG" 2>/dev/null || echo 0)
        fi
        
        if [ "$DONE" -gt "$LAST" ] 2>/dev/null; then
            echo "  Progress: $DONE/$BLOCKS_TO_SYNC blocks"
            LAST=$DONE
            
            if [ "$DONE" -ge "$BLOCKS_TO_SYNC" ]; then
                echo "  ✓ Done! Synced $DONE blocks"
                kill $PID 2>/dev/null || true
                break
            fi
        fi
    done
    
    wait $PID 2>/dev/null || true
    END=$(date +%s)
    DURATION=$((END - START))
    
    echo "$DURATION" > "time_$name.txt"
    echo "  Completed in: ${DURATION}s"
}

# Run tests
run_test "WITH_BATCHING" "$BATCH_SIZE" "./data_with_batching"
run_test "WITHOUT_BATCHING" "1" "./data_without_batching"

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
    echo "✓ BATCHING IS ${SPEEDUP}x FASTER ($PCT% improvement)"
else
    echo "✗ Batching is slower"
fi

echo ""
echo "========================================"
echo "DONE"
echo "========================================"

# Cleanup
rm -f test_config.json
