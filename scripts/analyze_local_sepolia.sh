#!/bin/bash

# Script to analyze LOCAL sepolia timing logs (similar to mainnet analysis)
# Usage: ./scripts/analyze_local_sepolia.sh

LOG_FILE="sepolia_timing_logs.txt"

if [ ! -f "$LOG_FILE" ]; then
    echo "No local sepolia logs found: $LOG_FILE"
    echo "Run: ./scripts/run_sepolia_node.sh 2>&1 | tee sepolia_timing_logs.txt"
    exit 1
fi

echo "========================================="
echo "LOCAL SEPOLIA TIMING ANALYSIS"
echo "========================================="
echo ""
echo "Analyzing: $LOG_FILE"
echo "File size: $(wc -l $LOG_FILE)"
echo ""

echo "=== CONSENSUS STATUS ==="
CONSENSUS_START=$(grep "Running consensus for height" "$LOG_FILE" | head -1 | sed 's/.*height \([0-9]*\).*/\1/' 2>/dev/null || echo "unknown")
if [ "$CONSENSUS_START" = "0" ]; then
    echo "Consensus starting from block 0 (correct)"
elif [ "$CONSENSUS_START" != "unknown" ]; then
    echo "Consensus starting from block $CONSENSUS_START"
else
    echo "Could not determine consensus starting point"
fi

echo ""
echo "=== BLOCK PROCESSING TIMES ==="

# Use the correct log pattern from sepolia logs
BLOCK_COUNT=$(grep -c "BATCHER_TIMING_END.*Total batcher processing took" "$LOG_FILE" 2>/dev/null || echo "0")
echo "Blocks processed: $BLOCK_COUNT"

if [ "$BLOCK_COUNT" -gt 0 ]; then
    # Get actual block range
    FIRST_BLOCK=$(grep 'BATCHER_TIMING_END.*Total batcher processing took' "$LOG_FILE" | head -1 | sed 's/.*Block \([0-9]*\).*/\1/' 2>/dev/null || echo "unknown")
    LAST_BLOCK=$(grep 'BATCHER_TIMING_END.*Total batcher processing took' "$LOG_FILE" | tail -1 | sed 's/.*Block \([0-9]*\).*/\1/' 2>/dev/null || echo "unknown")
    echo "Block range: $FIRST_BLOCK → $LAST_BLOCK"
    
    echo ""
    echo "Recent block processing times:"
    grep "BATCHER_TIMING_END.*Total batcher processing took" "$LOG_FILE" | tail -10 | while read line; do
        BLOCK=$(echo "$line" | sed 's/.*Block \([0-9]*\).*/\1/')
        TIME=$(echo "$line" | sed 's/.*took \([0-9.]*ms\).*/\1/')
        echo "Block $BLOCK: $TIME"
    done
    
    echo ""
    echo "=== LOOKING FOR INTER-BLOCK GAPS ==="
    python3 << 'PYTHON_EOF'
import re
from datetime import datetime

try:
    with open('sepolia_timing_logs.txt', 'r') as f:
        lines = f.readlines()
    
    timestamps = []
    for line in lines:
        if 'BATCHER_TIMING_END' in line and 'Total batcher processing took' in line:
            ts_match = re.search(r'(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z)', line)
            block_match = re.search(r'Block (\d+)', line)
            if ts_match and block_match:
                timestamps.append((int(block_match.group(1)), ts_match.group(1)))
    
    if len(timestamps) > 1:
        print("Inter-block timing gaps:")
        print("Block Range | Gap (ms) | Status")
        print("------------|----------|--------")
        
        large_gaps = []
        for i in range(1, min(len(timestamps), 21)):  # Show last 20 gaps
            prev_block, prev_ts = timestamps[i-1]
            curr_block, curr_ts = timestamps[i]
            
            prev_dt = datetime.fromisoformat(prev_ts.replace('Z', '+00:00'))
            curr_dt = datetime.fromisoformat(curr_ts.replace('Z', '+00:00'))
            gap_ms = (curr_dt - prev_dt).total_seconds() * 1000
            
            if gap_ms > 500:
                status = "700MS BOTTLENECK!"
                large_gaps.append((prev_block, curr_block, gap_ms))
            elif gap_ms > 100:
                status = "Elevated"
            else:
                status = "Normal"
            
            print(f"{prev_block:4d} → {curr_block:4d} | {gap_ms:7.1f} | {status}")
        
        if large_gaps:
            print(f"\nFOUND {len(large_gaps)} LARGE GAPS (>500ms):")
            for prev_block, curr_block, gap_ms in large_gaps:
                print(f"   Block {prev_block} → {curr_block}: {gap_ms:.1f}ms")
        else:
            print(f"\nNo large gaps found (all gaps < 500ms)")
            
    else:
        print("Not enough block data for gap analysis")
        
except Exception as e:
    print(f"Analysis error: {e}")
PYTHON_EOF

else
    echo "No block processing data found using BATCHER_TIMING_END pattern"
    echo ""
    echo "Let's check what timing patterns we actually have:"
    echo "BATCHER_TIMING_END: $(grep -c 'BATCHER_TIMING_END' "$LOG_FILE" 2>/dev/null || echo "0")"
    echo "ORCHESTRATOR_TIMING_END: $(grep -c 'ORCHESTRATOR_TIMING_END' "$LOG_FILE" 2>/dev/null || echo "0")"
    echo "SYNC_FLOW_TIMING: $(grep -c 'SYNC_FLOW_TIMING' "$LOG_FILE" 2>/dev/null || echo "0")"
fi

echo ""
echo "=== ORCHESTRATOR/BATCHER STATUS ==="
ORCH_COUNT=$(grep -c "ORCHESTRATOR_TIMING\|SYNC_FLOW_TIMING.*batcher" "$LOG_FILE" 2>/dev/null || echo "0")
echo "Orchestrator/Batcher logs: $ORCH_COUNT"

if [ "$ORCH_COUNT" -gt 0 ]; then
    echo ""
    echo "Orchestrator activity:"
    grep "ORCHESTRATOR_TIMING\|SYNC_FLOW_TIMING.*batcher" "$LOG_FILE" | head -5
else
    echo "No orchestrator/batcher activity yet"
    echo "   This suggests consensus hasn't reached the orchestrator stage"
    echo "   Try running longer or check if blocks are being processed"
fi

echo ""
echo "=== PERFORMANCE ANALYSIS ==="
if [ "$BLOCK_COUNT" -gt 0 ]; then
    # Calculate average processing time
    AVG_TIME=$(grep "BATCHER_TIMING_END.*Total batcher processing took" "$LOG_FILE" | \
               sed 's/.*took \([0-9.]*\)ms.*/\1/' | \
               awk '{sum+=$1; count++} END {if(count>0) printf "%.2f", sum/count; else print "0"}')
    echo "Average block processing time: ${AVG_TIME}ms"
    
    # Find slowest block
    SLOWEST=$(grep "BATCHER_TIMING_END.*Total batcher processing took" "$LOG_FILE" | \
              sed 's/.*Block \([0-9]*\).*took \([0-9.]*\)ms.*/\2 \1/' | \
              sort -nr | head -1)
    if [ -n "$SLOWEST" ]; then
        SLOWEST_TIME=$(echo "$SLOWEST" | cut -d' ' -f1)
        SLOWEST_BLOCK=$(echo "$SLOWEST" | cut -d' ' -f2)
        echo "Slowest block: Block $SLOWEST_BLOCK took ${SLOWEST_TIME}ms"
    fi
fi

echo ""
echo "DIAGNOSIS:"
if [ "$BLOCK_COUNT" -eq 0 ]; then
    echo "No blocks processed - sepolia sync may be very slow"
    echo "   Try running for 5-10 minutes to see block processing"
elif [ "$ORCH_COUNT" -eq 0 ]; then
    echo "Blocks processed but no orchestrator activity"
    echo "   This is expected during initial sync phase"
else
    echo "Both block processing and orchestrator activity detected"
fi

echo ""
echo "=== SEPOLIA vs MAINNET COMPARISON ==="
echo "Use this data to compare with mainnet timing analysis."
echo "Look for differences in:"
echo "  • Average block processing time"
echo "  • Inter-block gaps"
echo "  • Presence of 700ms+ bottlenecks"
