#!/bin/bash

# Script to analyze the batching test logs in detail
# Shows timing for each batch and overall statistics

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

LOG_WITH="${1:-test_with_batching.log}"
LOG_WITHOUT="${2:-test_without_batching.log}"

echo "========================================"
echo "BATCHING TEST DETAILED ANALYSIS"
echo "========================================"
echo ""

if [ ! -f "$LOG_WITH" ]; then
    echo -e "${RED}Error: Log file not found: $LOG_WITH${NC}"
    exit 1
fi

if [ ! -f "$LOG_WITHOUT" ]; then
    echo -e "${RED}Error: Log file not found: $LOG_WITHOUT${NC}"
    exit 1
fi

echo "Analyzing logs:"
echo "  • With batching: $LOG_WITH"
echo "  • Without batching: $LOG_WITHOUT"
echo ""

# Python script for detailed analysis
python3 << 'EOF'
import re
import sys
from datetime import datetime

def parse_rust_log_timestamp(line):
    """Extract timestamp from Rust log line"""
    # Format: "2025-11-05T10:30:45.123Z"
    match = re.match(r'^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z)', line)
    if match:
        timestamp_str = match.group(1)
        return datetime.fromisoformat(timestamp_str.replace('Z', '+00:00'))
    return None

def extract_block_number(line):
    """Extract block number from log line"""
    # Look for patterns like "block 12345" or "blocks 100 to 199"
    match = re.search(r'blocks?\s+(\d+)', line, re.IGNORECASE)
    if match:
        return int(match.group(1))
    
    # Look for BlockNumber(12345)
    match = re.search(r'BlockNumber\((\d+)\)', line)
    if match:
        return int(match.group(1))
    
    return None

def analyze_log(log_file, mode_name):
    """Analyze a single log file for batching performance"""
    print(f"\n{'='*60}")
    print(f"ANALYSIS: {mode_name}")
    print(f"{'='*60}\n")
    
    batch_times = []
    batch_starts = []
    batch_completions = []
    compilation_submissions = []
    
    with open(log_file, 'r') as f:
        for line in f:
            timestamp = parse_rust_log_timestamp(line)
            if not timestamp:
                continue
            
            # Track compilation task submissions
            if 'Compilation task submitted for block' in line:
                match = re.search(r'Batch count: (\d+)/(\d+)', line)
                if match:
                    count = int(match.group(1))
                    compilation_submissions.append((timestamp, count))
            
            # Track batch collection starts
            if 'Batch size reached' in line or 'Collecting compiled results' in line:
                match = re.search(r'(\d+) blocks', line)
                if match:
                    batch_starts.append((timestamp, int(match.group(1))))
            
            # Track batch write completions
            if 'Successfully wrote' in line and 'blocks to storage in one transaction' in line:
                match = re.search(r'Successfully wrote (\d+) blocks', line)
                if match:
                    blocks = int(match.group(1))
                    batch_completions.append((timestamp, blocks))
    
    # Calculate batch processing times
    for i in range(min(len(batch_starts), len(batch_completions))):
        start_time, start_blocks = batch_starts[i]
        end_time, end_blocks = batch_completions[i]
        
        if start_blocks == end_blocks or abs(start_blocks - end_blocks) < 50:
            duration_ms = (end_time - start_time).total_seconds() * 1000
            batch_times.append((i+1, start_blocks, duration_ms))
    
    if not batch_times:
        print(f"No batch timing data found in {log_file}")
        print("This might indicate:")
        print("  • The sync didn't complete any batches")
        print("  • Log format has changed")
        print("  • Batching is disabled")
        return None
    
    # Calculate statistics
    times_only = [t[2] for t in batch_times]
    avg_time = sum(times_only) / len(times_only)
    min_time = min(times_only)
    max_time = max(times_only)
    
    print(f"Batches Completed: {len(batch_times)}")
    print(f"Total Blocks Synced: {sum(t[1] for t in batch_times)}")
    print(f"\nBATCH TIMING STATISTICS:")
    print(f"  • Average: {avg_time:.0f}ms per batch")
    print(f"  • Min: {min_time:.0f}ms")
    print(f"  • Max: {max_time:.0f}ms")
    print(f"  • Range: {max_time - min_time:.0f}ms")
    
    # Show per-batch breakdown
    print(f"\nPER-BATCH BREAKDOWN:")
    print(f"{'Batch':<8} {'Blocks':<8} {'Time (ms)':<12} {'Blocks/sec':<12}")
    print(f"{'-'*45}")
    
    for batch_num, blocks, duration_ms in batch_times[:20]:  # Show first 20
        blocks_per_sec = (blocks / (duration_ms / 1000.0)) if duration_ms > 0 else 0
        print(f"{batch_num:<8} {blocks:<8} {duration_ms:<12.0f} {blocks_per_sec:<12.1f}")
    
    if len(batch_times) > 20:
        print(f"... and {len(batch_times) - 20} more batches")
    
    # Calculate total time
    if batch_times:
        first_start = batch_starts[0][0]
        last_end = batch_completions[len(batch_times)-1][0]
        total_duration = (last_end - first_start).total_seconds()
        total_blocks = sum(t[1] for t in batch_times)
        overall_rate = total_blocks / total_duration if total_duration > 0 else 0
        
        print(f"\nOVERALL PERFORMANCE:")
        print(f"  • Total time: {total_duration:.1f}s")
        print(f"  • Total blocks: {total_blocks}")
        print(f"  • Average rate: {overall_rate:.1f} blocks/sec")
    
    return {
        'batch_count': len(batch_times),
        'total_blocks': sum(t[1] for t in batch_times),
        'avg_batch_time': avg_time,
        'min_batch_time': min_time,
        'max_batch_time': max_time,
        'total_time': total_duration if batch_times else 0
    }

# Analyze both logs
with_batching = analyze_log(sys.argv[1], "WITH BATCHING (FuturesOrdered)")
without_batching = analyze_log(sys.argv[2], "WITHOUT BATCHING (Sequential)")

# Compare results
if with_batching and without_batching:
    print(f"\n{'='*60}")
    print(f"COMPARISON")
    print(f"{'='*60}\n")
    
    print(f"{'Metric':<30} {'With Batching':<20} {'Without Batching':<20}")
    print(f"{'-'*70}")
    
    print(f"{'Batches completed':<30} {with_batching['batch_count']:<20} {without_batching['batch_count']:<20}")
    print(f"{'Total blocks synced':<30} {with_batching['total_blocks']:<20} {without_batching['total_blocks']:<20}")
    print(f"{'Average batch time':<30} {with_batching['avg_batch_time']:.0f}ms{'':<17} {without_batching['avg_batch_time']:.0f}ms")
    print(f"{'Total time':<30} {with_batching['total_time']:.1f}s{'':<17} {without_batching['total_time']:.1f}s")
    
    if with_batching['total_time'] > 0 and without_batching['total_time'] > 0:
        speedup = (without_batching['total_time'] / with_batching['total_time'])
        improvement_pct = ((without_batching['total_time'] - with_batching['total_time']) / without_batching['total_time']) * 100
        
        print(f"\n{'RESULTS:':<30}")
        if speedup > 1:
            print(f"  ✓ Batching is {speedup:.2f}x FASTER")
            print(f"  ✓ {improvement_pct:.1f}% improvement")
        else:
            print(f"  ✗ Batching is {1/speedup:.2f}x SLOWER")
            print(f"  ✗ {abs(improvement_pct):.1f}% regression")
        
        rate_with = with_batching['total_blocks'] / with_batching['total_time']
        rate_without = without_batching['total_blocks'] / without_batching['total_time']
        
        print(f"\nTHROUGHPUT:")
        print(f"  • With batching: {rate_with:.1f} blocks/sec")
        print(f"  • Without batching: {rate_without:.1f} blocks/sec")

print(f"\n{'='*60}")
print(f"ANALYSIS COMPLETE")
print(f"{'='*60}")

EOF

echo ""
echo "For more details, check the full log files:"
echo "  • $LOG_WITH"
echo "  • $LOG_WITHOUT"

