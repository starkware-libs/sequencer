#!/bin/bash

# Clean script to measure end-to-end block processing time
# From: Feeder Gateway Success → Batcher Storage Completion
# Usage: ./scripts/measure_block_end_to_end.sh [log_file] [num_blocks_to_show]

LOG_FILE="${1:-mainnet_timing_logs.txt}"
NUM_BLOCKS="${2:-20}"

echo "========================================"
echo "BLOCK END-TO-END TIMING MEASUREMENT"
echo "========================================"
echo ""
echo "Measuring: Feeder Gateway Success → Batcher Storage Completion"
echo "Log file: $LOG_FILE"
echo "Showing: Last $NUM_BLOCKS blocks"
echo ""

if [ ! -f "$LOG_FILE" ]; then
    echo "Log file not found: $LOG_FILE"
    echo ""
    echo "Usage: $0 [log_file] [num_blocks_to_show]"
    echo "Example: $0 mainnet_timing_logs.txt 10"
    exit 1
fi

python3 << PYTHON_EOF
import re
from datetime import datetime
import sys

def parse_timestamp(ts_str):
    """Parse ISO timestamp to datetime object"""
    return datetime.fromisoformat(ts_str.replace('Z', '+00:00'))

def format_time_diff(ms):
    """Format milliseconds in a readable way"""
    if ms < 1000:
        return f"{ms:.0f}ms"
    elif ms < 60000:
        return f"{ms/1000:.1f}s"
    else:
        minutes = int(ms / 60000)
        seconds = (ms % 60000) / 1000
        return f"{minutes}m{seconds:.1f}s"

try:
    with open('$LOG_FILE', 'r') as f:
        lines = f.readlines()
    
    print("Scanning logs...")
    
    # Extract feeder gateway successes and batcher completions
    feeder_successes = {}  # block_num -> timestamp
    batcher_completions = {}  # block_num -> timestamp
    
    feeder_pattern = re.compile(r'Call to feeder succeeded.*get_block.*blockNumber=(\d+)')
    batcher_pattern = re.compile(r'BATCHER_TIMING_END.*Block (\d+)')
    timestamp_pattern = re.compile(r'(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z)')
    
    for line in lines:
        ts_match = timestamp_pattern.search(line)
        if not ts_match:
            continue
        timestamp = ts_match.group(1)
        
        # Check for feeder gateway success
        feeder_match = feeder_pattern.search(line)
        if feeder_match:
            block_num = int(feeder_match.group(1))
            if block_num not in feeder_successes:  # Take first success for each block
                feeder_successes[block_num] = timestamp
        
        # Check for batcher completion
        batcher_match = batcher_pattern.search(line)
        if batcher_match:
            block_num = int(batcher_match.group(1))
            batcher_completions[block_num] = timestamp
    
    print(f"Found {len(feeder_successes)} feeder successes")
    print(f"Found {len(batcher_completions)} batcher completions")
    
    # Find blocks with complete data
    complete_blocks = set(feeder_successes.keys()) & set(batcher_completions.keys())
    
    if not complete_blocks:
        print("No blocks found with both feeder success and batcher completion")
        print("   This could mean:")
        print("   • Logs don't cover the complete pipeline")
        print("   • Different log levels for different components")
        print("   • Batcher is significantly behind feeder")
        sys.exit(1)
    
    print(f"Found {len(complete_blocks)} blocks with complete pipeline data")
    print("")
    
    # Calculate end-to-end times
    timing_results = []
    
    for block_num in complete_blocks:
        try:
            feeder_time = parse_timestamp(feeder_successes[block_num])
            batcher_time = parse_timestamp(batcher_completions[block_num])
            
            # Calculate end-to-end time in milliseconds
            end_to_end_ms = (batcher_time - feeder_time).total_seconds() * 1000
            
            # Only include positive times (batcher should be after feeder)
            if end_to_end_ms > 0:
                timing_results.append((block_num, end_to_end_ms, feeder_successes[block_num], batcher_completions[block_num]))
        except:
            continue
    
    if not timing_results:
        print("Could not calculate valid end-to-end times")
        sys.exit(1)
    
    # Sort by block number
    timing_results.sort()
    
    # Show results
    print("Block-by-block end-to-end timing:")
    print("=" * 80)
    print(f"{'Block':>6} | {'End-to-End Time':>15} | {'Feeder Success':>20} | {'Batcher Complete':>20}")
    print("-" * 80)
    
    # Show last N blocks
    recent_blocks = timing_results[-$NUM_BLOCKS:]
    
    total_time = 0
    for block_num, end_to_end_ms, feeder_ts, batcher_ts in recent_blocks:
        total_time += end_to_end_ms
        
        # Format timestamps to show just time
        feeder_time_str = feeder_ts[11:23]  # Extract HH:MM:SS.mmm
        batcher_time_str = batcher_ts[11:23]
        
        # Color coding for time
        if end_to_end_ms >= 10000:  # 10+ seconds
            status = "High latency"
        elif end_to_end_ms >= 1000:  # 1+ second
            status = "Elevated"
        else:
            status = "Normal"
        
        print(f"{block_num:6d} | {format_time_diff(end_to_end_ms):>15} | {feeder_time_str:>20} | {batcher_time_str:>20} {status}")
    
    print("-" * 80)
    
    # Summary statistics
    if recent_blocks:
        avg_time = total_time / len(recent_blocks)
        min_time = min(t[1] for t in recent_blocks)
        max_time = max(t[1] for t in recent_blocks)
        
        print("")
        print("Summary statistics:")
        print("=" * 50)
        print(f"Blocks analyzed:     {len(recent_blocks)}")
        print(f"Average time:        {format_time_diff(avg_time)}")
        print(f"Minimum time:        {format_time_diff(min_time)}")
        print(f"Maximum time:        {format_time_diff(max_time)}")
        
        # Performance assessment
        print("")
        print("PERFORMANCE ASSESSMENT:")
        if avg_time > 5000:  # 5+ seconds
            print("Very slow: Average >5 seconds per block")
            print("   Your system is significantly bottlenecked")
        elif avg_time > 1000:  # 1+ second
            print("Slow: Average >1 second per block")
            print("   There are performance issues to address")
        elif avg_time > 700:  # Target threshold
            print("Above target: Average >700ms per block")
            print("   Close to target but could be optimized")
        else:
            print("Good: Average <700ms per block")
            print("   Performance meets target")
    
    print("")
    print("Interpretation:")
    print("• This measures TRUE user experience latency")
    print("• From block download to final storage completion")
    print("• Includes all pipeline delays and bottlenecks")
    
except Exception as e:
    print(f"Error: {e}")
    import traceback
    traceback.print_exc()
PYTHON_EOF

echo ""
echo "========================================"
echo "HOW TO USE THIS SCRIPT:"
echo ""
echo "1. Run your node and save logs:"
echo "   ./scripts/run_mainnet_node.sh 2>&1 | tee timing_logs.txt"
echo ""
echo "2. Measure timing (default: last 20 blocks):"
echo "   ./scripts/measure_block_end_to_end.sh timing_logs.txt"
echo ""
echo "3. Show more/fewer blocks:"
echo "   ./scripts/measure_block_end_to_end.sh timing_logs.txt 50"
echo ""
echo "4. Monitor in real-time:"
echo "   watch -n 10 './scripts/measure_block_end_to_end.sh timing_logs.txt 10'"


//this is the new script instead of the old end to end- make sure i doc this and also add to the branch and make it look better.
