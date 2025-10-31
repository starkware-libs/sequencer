#!/bin/bash

# Script to test the impact of different MAX_READERS values on sync performance
set -e

echo "MAX_READERS Impact Testing"
echo "============================="
echo ""

# Test configurations
READERS_HIGH=8192    # Current value (1 << 13)
READERS_LOW=512      # Reduced value (1 << 9)
TEST_BLOCKS=100      # Number of blocks to sync for each test

echo "Test Plan:"
echo "  - Test 1: MAX_READERS = $READERS_HIGH (current)"
echo "  - Test 2: MAX_READERS = $READERS_LOW (reduced)"
echo "  - Measure: First $TEST_BLOCKS blocks sync time"
echo ""

# Function to run a test
run_test() {
    local readers=$1
    local test_name=$2
    local log_file="timing_readers_${readers}.txt"
    
    echo "Running Test: $test_name (MAX_READERS=$readers)"
    
    # Update the code
    sed -i "s/const MAX_READERS: u32 = 1 << [0-9]*;/const MAX_READERS: u32 = $readers;/" crates/apollo_storage/src/db/mod.rs
    
    # Build
    echo "  Building..."
    cargo build --release --bin apollo_node > /dev/null 2>&1
    
    # Reset state
    echo "  Resetting node state..."
    ./scripts/reset_node_state.sh <<< "y" > /dev/null 2>&1
    
    # Run sync and capture timing
    echo "  Starting sync (capturing first $TEST_BLOCKS blocks)..."
    timeout 300s bash -c "
        RUST_LOG=debug ./scripts/run_sepolia_node.sh 2>&1 | 
        grep -E '(BLOCK_TIMING|STORAGE_TIMING|COMMIT_TIMING)' --line-buffered |
        head -$(($TEST_BLOCKS * 6)) > $log_file
    " || echo "  Timeout reached or sync completed"
    
    # Stop any remaining processes
    pkill -f "apollo_node" || true
    sleep 2
    
    echo "Test completed, logs saved to $log_file"
    echo ""
}

# Backup original code
cp crates/apollo_storage/src/db/mod.rs crates/apollo_storage/src/db/mod.rs.backup

echo "Starting tests..."
echo ""

# Test 1: High readers (current)
run_test $READERS_HIGH "High Readers"

# Test 2: Low readers
run_test $READERS_LOW "Low Readers"

# Restore original code
mv crates/apollo_storage/src/db/mod.rs.backup crates/apollo_storage/src/db/mod.rs

echo "Analysis Results:"
echo "==================="

# Analyze results
for readers in $READERS_HIGH $READERS_LOW; do
    log_file="timing_readers_${readers}.txt"
    if [ -f "$log_file" ]; then
        echo ""
        echo "MAX_READERS = $readers:"
        ./scripts/analyze_timing.sh "$log_file" | grep -E "(Average|Blocks analyzed|improvement)"
    fi
done

echo ""
echo "Detailed logs available:"
echo "  - timing_readers_${READERS_HIGH}.txt (high readers)"
echo "  - timing_readers_${READERS_LOW}.txt (low readers)"
echo ""
echo "For detailed comparison:"
echo "  ./scripts/analyze_timing.sh timing_readers_${READERS_HIGH}.txt"
echo "  ./scripts/analyze_timing.sh timing_readers_${READERS_LOW}.txt"
