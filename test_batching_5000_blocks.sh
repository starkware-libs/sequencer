#!/bin/bash

# Test batching by syncing 5000 blocks (5 batches of 1000)
# This will stop automatically when target is reached

set -e

TARGET_BLOCKS=5000
LOG_FILE="batching_test.log"

echo "=========================================="
echo "Batching Test - Sync $TARGET_BLOCKS blocks"
echo "=========================================="
echo ""

# Kill any existing nodes
echo "Stopping any running nodes..."
pkill -f apollo_node 2>/dev/null || true
sleep 2

# Verify config has batching enabled
echo "Verifying configuration..."
BATCH_SIZE=$(grep "block_batch_size" crates/apollo_deployments/resources/app_configs/state_sync_config.json | grep -o '[0-9]*')
BATCHING_ENABLED=$(grep "enable_block_batching" crates/apollo_deployments/resources/app_configs/state_sync_config.json | grep -o 'true\|false')

echo "  enable_block_batching: $BATCHING_ENABLED"
echo "  block_batch_size: $BATCH_SIZE"
echo ""

if [ "$BATCHING_ENABLED" != "true" ] || [ "$BATCH_SIZE" != "1000" ]; then
    echo "ERROR: Batching not configured correctly!"
    echo "Expected: enable_block_batching=true, block_batch_size=1000"
    exit 1
fi

# Start the node
echo "Starting node with batching enabled..."
bash scripts/run_mainnet_node.sh > "$LOG_FILE" 2>&1 &
NODE_PID=$!
echo "Node started with PID: $NODE_PID"
echo ""

# Wait for node to initialize
echo "Waiting 60 seconds for node to initialize..."
sleep 60

# Check if node is still running
if ! ps -p $NODE_PID > /dev/null 2>&1; then
    echo "ERROR: Node failed to start! Check $LOG_FILE"
    exit 1
fi

echo "Node is running. Monitoring sync progress..."
echo "Will stop after syncing $TARGET_BLOCKS new blocks"
echo ""

# Get initial block count
INITIAL_BLOCKS=$(grep -a "SYNC_NEW_BLOCK" "$LOG_FILE" 2>/dev/null | wc -l)
echo "Starting block count: $INITIAL_BLOCKS"
echo ""

START_TIME=$(date +%s)

# Monitor progress
while true; do
    sleep 30
    
    # Check if node is still running
    if ! ps -p $NODE_PID > /dev/null 2>&1; then
        echo "Node stopped unexpectedly! Check $LOG_FILE"
        exit 1
    fi
    
    CURRENT_BLOCKS=$(grep -a "SYNC_NEW_BLOCK" "$LOG_FILE" 2>/dev/null | wc -l)
    NEW_BLOCKS=$((CURRENT_BLOCKS - INITIAL_BLOCKS))
    ELAPSED=$(($(date +%s) - START_TIME))
    
    # Calculate rate
    if [ $ELAPSED -gt 0 ]; then
        RATE=$(echo "scale=2; $NEW_BLOCKS / $ELAPSED" | bc 2>/dev/null || echo "0")
    else
        RATE="0.00"
    fi
    
    # Get current block/state diff numbers being processed
    LATEST_BLOCK=$(grep -a "Received new block [0-9]" "$LOG_FILE" 2>/dev/null | tail -1 | grep -o "block [0-9]*" | grep -o "[0-9]*")
    LATEST_STATE=$(grep -a "Received new state update of block [0-9]" "$LOG_FILE" 2>/dev/null | tail -1 | grep -o "block [0-9]*" | grep -o "[0-9]*")
    
    echo "[${ELAPSED}s] Committed: $NEW_BLOCKS blocks | Downloading: block $LATEST_BLOCK, state_diff $LATEST_STATE"
    
    # Check for batch timing logs
    BATCH_COUNT=$(grep -a "BATCH_TIMING_START" "$LOG_FILE" 2>/dev/null | wc -l)
    if [ $BATCH_COUNT -gt 0 ]; then
        echo "       ✓ Batches flushed: $BATCH_COUNT"
    fi
    
    # Stop when we reach target
    if [ $NEW_BLOCKS -ge $TARGET_BLOCKS ]; then
        echo ""
        echo "=========================================="
        echo "TARGET REACHED: $NEW_BLOCKS blocks synced!"
        echo "=========================================="
        break
    fi
done

# Stop the node
echo ""
echo "Stopping node..."
kill $NODE_PID 2>/dev/null || true
sleep 2

# Final statistics
FINAL_BLOCKS=$(grep -a "SYNC_NEW_BLOCK" "$LOG_FILE" 2>/dev/null | wc -l)
TOTAL_NEW=$((FINAL_BLOCKS - INITIAL_BLOCKS))
ELAPSED=$(($(date +%s) - START_TIME))

echo ""
echo "=========================================="
echo "FINAL RESULTS"
echo "=========================================="
echo "Total blocks synced: $TOTAL_NEW"
echo "Time elapsed: $ELAPSED seconds"
if [ $ELAPSED -gt 0 ]; then
    AVG_RATE=$(echo "scale=2; $TOTAL_NEW / $ELAPSED" | bc 2>/dev/null || echo "0")
    echo "Average rate: $AVG_RATE blocks/sec"
fi
echo ""

# Show batch timing samples
echo "Sample batch timing logs:"
grep -a "BATCH_TIMING:" "$LOG_FILE" 2>/dev/null | head -5 || echo "  (No batch timing logs found yet)"
echo ""

echo "✓ Test complete! Log saved to: $LOG_FILE"
echo ""
echo "To view detailed logs:"
echo "  grep -a 'BATCH_TIMING' $LOG_FILE"
echo "  grep -a 'SYNC_NEW_BLOCK' $LOG_FILE | tail -20"

