#!/bin/bash

# Compare batching performance: WITH batching vs WITHOUT batching
# Tests a smaller sample (1000 blocks) for faster results

set -e

TARGET_BLOCKS=1000
# Use temporary storage for fair comparison (both tests start from block 0)
STORAGE_BATCHING="./data_compare_batching"
STORAGE_NO_BATCHING="./data_compare_no_batching"

echo "=========================================="
echo "Batching Performance Comparison"
echo "=========================================="
echo "Testing $TARGET_BLOCKS blocks each"
echo ""

# Kill any existing nodes
echo "Stopping any running nodes..."
pkill -f apollo_node 2>/dev/null || true
sleep 2

# Backup current config
cp crates/apollo_deployments/resources/app_configs/state_sync_config.json /tmp/config_backup.json

#############################################
# TEST 1: WITH BATCHING (batch_size=1000)
#############################################

echo ""
echo "=========================================="
echo "TEST 1: WITH BATCHING (batch_size=1000)"
echo "=========================================="

# Create fresh storage for this test
rm -rf "$STORAGE_BATCHING"
mkdir -p "$STORAGE_BATCHING"
echo "Using fresh storage: $STORAGE_BATCHING (starting from block 0)"
echo ""

# Ensure batching is enabled with size 1000
cat > /tmp/test_config.json << EOF
{
  "state_sync_config.central_sync_client_config.sync_config.enable_block_batching": true,
  "state_sync_config.central_sync_client_config.sync_config.block_batch_size": 1000,
  "state_sync_config.storage_config.db_config.path_prefix": "$STORAGE_BATCHING"
}
EOF

jq -s '.[0] * .[1]' /tmp/config_backup.json /tmp/test_config.json > crates/apollo_deployments/resources/app_configs/state_sync_config.json

echo "Starting node with batching (batch_size=1000)..."
bash scripts/run_mainnet_node.sh > test_batching_on.log 2>&1 &
NODE_PID=$!
echo "Node PID: $NODE_PID"

# Wait for node to start
sleep 60

if ! ps -p $NODE_PID > /dev/null 2>&1; then
    echo "ERROR: Node failed to start! Check test_batching_on.log"
    exit 1
fi

echo "Node running. Waiting for $TARGET_BLOCKS blocks..."

INITIAL_BLOCKS=0
START_TIME=$(date +%s)

# Monitor until target reached
while true; do
    sleep 15
    
    if ! ps -p $NODE_PID > /dev/null 2>&1; then
        echo "ERROR: Node stopped! Check test_batching_on.log"
        exit 1
    fi
    
    CURRENT_BLOCKS=$(grep -ac "SYNC_NEW_BLOCK" test_batching_on.log 2>/dev/null | head -1 || echo "0")
    NEW_BLOCKS=$((CURRENT_BLOCKS - INITIAL_BLOCKS))
    ELAPSED=$(($(date +%s) - START_TIME))
    
    if [ $ELAPSED -gt 0 ]; then
        RATE=$(echo "scale=2; $NEW_BLOCKS / $ELAPSED" | bc 2>/dev/null || echo "0")
    else
        RATE="0.00"
    fi
    
    echo "  [$ELAPSED sec] Synced: $NEW_BLOCKS blocks (Rate: $RATE blocks/sec)"
    
    if [ $NEW_BLOCKS -ge $TARGET_BLOCKS ]; then
        break
    fi
    
    # Safety timeout: 20 minutes
    if [ $ELAPSED -ge 1200 ]; then
        echo "WARNING: Timeout reached (20 minutes). Stopping test."
        break
    fi
done

# Stop node
echo "Stopping test 1 node..."
kill $NODE_PID 2>/dev/null || pkill -f apollo_node || true
sleep 5

END_TIME=$(date +%s)
DURATION_WITH_BATCHING=$((END_TIME - START_TIME))
BLOCKS_WITH_BATCHING=$(grep -ac "SYNC_NEW_BLOCK" test_batching_on.log 2>/dev/null | head -1 || echo "0")
BATCHES_WITH=$(grep -ac "BATCH_TIMING_START" test_batching_on.log 2>/dev/null | head -1 || echo "0")

echo ""
echo "TEST 1 COMPLETE:"
echo "  Blocks synced: $BLOCKS_WITH_BATCHING"
echo "  Time: $DURATION_WITH_BATCHING seconds"
echo "  Batches: $BATCHES_WITH"
if [ $DURATION_WITH_BATCHING -gt 0 ]; then
    RATE_WITH=$(echo "scale=2; $BLOCKS_WITH_BATCHING / $DURATION_WITH_BATCHING" | bc 2>/dev/null || echo "0")
    echo "  Rate: $RATE_WITH blocks/sec"
fi

#############################################
# TEST 2: WITHOUT BATCHING (batch_size=1)
#############################################

echo ""
echo "=========================================="
echo "TEST 2: WITHOUT BATCHING (batch_size=1)"
echo "=========================================="

# Create fresh storage for this test
rm -rf "$STORAGE_NO_BATCHING"
mkdir -p "$STORAGE_NO_BATCHING"
echo "Using fresh storage: $STORAGE_NO_BATCHING (starting from block 0)"
echo ""

# Configure batch_size=1 (effectively no batching)
cat > /tmp/test_config.json << EOF
{
  "state_sync_config.central_sync_client_config.sync_config.enable_block_batching": true,
  "state_sync_config.central_sync_client_config.sync_config.block_batch_size": 1,
  "state_sync_config.storage_config.db_config.path_prefix": "$STORAGE_NO_BATCHING"
}
EOF

jq -s '.[0] * .[1]' /tmp/config_backup.json /tmp/test_config.json > crates/apollo_deployments/resources/app_configs/state_sync_config.json

echo "Starting node WITHOUT batching (batch_size=1)..."
bash scripts/run_mainnet_node.sh > test_batching_off.log 2>&1 &
NODE_PID=$!
echo "Node PID: $NODE_PID"

# Wait for node to start
sleep 60

if ! ps -p $NODE_PID > /dev/null 2>&1; then
    echo "ERROR: Node failed to start! Check test_batching_off.log"
    exit 1
fi

echo "Node running. Waiting for $TARGET_BLOCKS blocks..."

INITIAL_BLOCKS=0
START_TIME=$(date +%s)

# Monitor until target reached
while true; do
    sleep 15
    
    if ! ps -p $NODE_PID > /dev/null 2>&1; then
        echo "ERROR: Node stopped! Check test_batching_off.log"
        exit 1
    fi
    
    CURRENT_BLOCKS=$(grep -ac "SYNC_NEW_BLOCK" test_batching_off.log 2>/dev/null | head -1 || echo "0")
    NEW_BLOCKS=$((CURRENT_BLOCKS - INITIAL_BLOCKS))
    ELAPSED=$(($(date +%s) - START_TIME))
    
    if [ $ELAPSED -gt 0 ]; then
        RATE=$(echo "scale=2; $NEW_BLOCKS / $ELAPSED" | bc 2>/dev/null || echo "0")
    else
        RATE="0.00"
    fi
    
    echo "  [$ELAPSED sec] Synced: $NEW_BLOCKS blocks (Rate: $RATE blocks/sec)"
    
    if [ $NEW_BLOCKS -ge $TARGET_BLOCKS ]; then
        break
    fi
    
    # Safety timeout: 20 minutes
    if [ $ELAPSED -ge 1200 ]; then
        echo "WARNING: Timeout reached (20 minutes). Stopping test."
        break
    fi
done

# Stop node
echo "Stopping test 2 node..."
kill $NODE_PID 2>/dev/null || pkill -f apollo_node || true
sleep 5

END_TIME=$(date +%s)
DURATION_NO_BATCHING=$((END_TIME - START_TIME))
BLOCKS_NO_BATCHING=$(grep -ac "SYNC_NEW_BLOCK" test_batching_off.log 2>/dev/null | head -1 || echo "0")
BATCHES_WITHOUT=$(grep -ac "BATCH_TIMING_START" test_batching_off.log 2>/dev/null | head -1 || echo "0")

echo ""
echo "TEST 2 COMPLETE:"
echo "  Blocks synced: $BLOCKS_NO_BATCHING"
echo "  Time: $DURATION_NO_BATCHING seconds"
echo "  Batches: $BATCHES_WITHOUT"
if [ $DURATION_NO_BATCHING -gt 0 ]; then
    RATE_WITHOUT=$(echo "scale=2; $BLOCKS_NO_BATCHING / $DURATION_NO_BATCHING" | bc 2>/dev/null || echo "0")
    echo "  Rate: $RATE_WITHOUT blocks/sec"
fi

# Restore original config
cp /tmp/config_backup.json crates/apollo_deployments/resources/app_configs/state_sync_config.json

#############################################
# COMPARISON RESULTS
#############################################

echo ""
echo "=========================================="
echo "         COMPARISON RESULTS"
echo "=========================================="
echo ""
echo "WITH BATCHING (batch_size=1000):"
echo "  Blocks: $BLOCKS_WITH_BATCHING"
echo "  Time: $DURATION_WITH_BATCHING seconds"
echo "  Batches: $BATCHES_WITH"
if [ $DURATION_WITH_BATCHING -gt 0 ]; then
    echo "  Rate: $(echo "scale=2; $BLOCKS_WITH_BATCHING / $DURATION_WITH_BATCHING" | bc) blocks/sec"
fi
echo ""
echo "WITHOUT BATCHING (batch_size=1):"
echo "  Blocks: $BLOCKS_NO_BATCHING"
echo "  Time: $DURATION_NO_BATCHING seconds"
echo "  Batches: $BATCHES_WITHOUT"
if [ $DURATION_NO_BATCHING -gt 0 ]; then
    echo "  Rate: $(echo "scale=2; $BLOCKS_NO_BATCHING / $DURATION_NO_BATCHING" | bc) blocks/sec"
fi
echo ""

# Calculate improvement
if [ $DURATION_NO_BATCHING -gt 0 ] && [ $DURATION_WITH_BATCHING -gt 0 ]; then
    TIME_SAVED=$((DURATION_NO_BATCHING - DURATION_WITH_BATCHING))
    PERCENT_IMPROVEMENT=$(echo "scale=2; ($TIME_SAVED * 100) / $DURATION_NO_BATCHING" | bc)
    
    echo "IMPROVEMENT:"
    echo "  Time saved: $TIME_SAVED seconds"
    echo "  Speedup: ${PERCENT_IMPROVEMENT}%"
    
    if [ $TIME_SAVED -gt 0 ]; then
        echo "  ✅ Batching is FASTER!"
    elif [ $TIME_SAVED -lt 0 ]; then
        echo "  ⚠️  Batching is slower (but this might be due to network variance)"
    else
        echo "  ≈ Similar performance"
    fi
fi

echo ""
echo "Logs saved to:"
echo "  - test_batching_on.log"
echo "  - test_batching_off.log"
echo ""
echo "Test storage directories:"
echo "  - $STORAGE_BATCHING"
echo "  - $STORAGE_NO_BATCHING"
echo ""
echo "To clean up test storage:"
echo "  rm -rf $STORAGE_BATCHING $STORAGE_NO_BATCHING"
echo ""
echo "✓ Comparison complete!"

