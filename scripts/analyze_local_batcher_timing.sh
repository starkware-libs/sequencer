#!/bin/bash

# Script to analyze Batcher processing timing from local logs
# Measures: BATCHER_TIMING_START → BATCHER_TIMING_END

set -e

LOG_FILE="${1:-mainnet_timing_logs.txt}"
MODE="${2:-detailed}"

echo "========================================"
echo "LOCAL BATCHER TIMING ANALYSIS"
echo "========================================"
echo ""
echo "Analyzing: $LOG_FILE"
echo "Mode: $MODE"
echo ""

if [ ! -f "$LOG_FILE" ]; then
    echo "Log file not found: $LOG_FILE"
    exit 1
fi

echo "EXTRACTING LOCAL BATCHER TIMING LOGS..."

# Extract batcher start logs
echo "Looking for Batcher start logs..."
grep 'BATCHER_TIMING_START: Processing sync block for height' "$LOG_FILE" > batcher_starts_local.txt 2>/dev/null || {
    echo "No 'BATCHER_TIMING_START' logs found!"
    echo ""
    echo "Expected log pattern:"
    echo "   'BATCHER_TIMING_START: Processing sync block for height XXXXX'"
    echo ""
    echo "Let me check what batcher logs are available..."
    grep -i "batcher.*timing" "$LOG_FILE" | head -5
    exit 1
}

# Extract batcher end logs
echo "Looking for Batcher end logs..."
grep 'BATCHER_TIMING_END.*Total batcher processing took' "$LOG_FILE" > batcher_ends_local.txt 2>/dev/null || {
    echo "No 'BATCHER_TIMING_END' logs found!"
    echo ""
    echo "Expected log pattern:"
    echo "   'BATCHER_TIMING_END: Block XXX - Total batcher processing took'"
    echo ""
    echo "Let me check what batcher end logs are available..."
    grep -i "batcher.*end" "$LOG_FILE" | head -5
    exit 1
}

START_COUNT=$(wc -l < batcher_starts_local.txt)
END_COUNT=$(wc -l < batcher_ends_local.txt)

echo "Found $START_COUNT batcher start logs"
echo "Found $END_COUNT batcher end logs"

if [ "$START_COUNT" -eq 0 ] || [ "$END_COUNT" -eq 0 ]; then
    echo "Need both start and end logs to calculate timing"
    exit 1
fi

echo ""
echo "CALCULATING BATCHER PROCESSING TIMES..."

python3 << 'EOF'
import re
from datetime import datetime
import statistics

def parse_timestamp(timestamp_str):
    """Parse local log timestamp format"""
    try:
        return datetime.fromisoformat(timestamp_str.replace('Z', '+00:00'))
    except:
        return None

def extract_block_number(text):
    """Extract block number from log message"""
    # Handle local format: "height 12345" or "Block 12345"
    matches = re.findall(r'height (\d+)', text)
    if matches:
        return int(matches[0])
    matches = re.findall(r'Block (\d+)', text)
    if matches:
        return int(matches[0])
    return None

def extract_processing_time(text):
    """Extract processing time from BATCHER_TIMING_END log"""
    # Handle format: "Total batcher processing took 4.503217ms"
    matches = re.findall(r'processing took ([0-9.]+)ms', text)
    if matches:
        return float(matches[0])
    return None

# Parse batcher starts
print("Parsing batcher start logs...")
batcher_starts = []

with open('batcher_starts_local.txt', 'r') as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
            
        # Remove ANSI escape sequences
        clean_line = re.sub(r'\x1b\[[0-9;]*m', '', line)
        
        # Extract timestamp
        timestamp_match = re.match(r'^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z)', clean_line)
        block_num = extract_block_number(clean_line)
        
        if timestamp_match and block_num is not None:
            timestamp = parse_timestamp(timestamp_match.group(1))
            if timestamp:
                batcher_starts.append((timestamp, block_num))

print(f"Parsed {len(batcher_starts)} batcher start entries")

# Parse batcher ends
print("Parsing batcher end logs...")
batcher_ends = []
processing_times_direct = []  # Direct from log messages

with open('batcher_ends_local.txt', 'r') as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
            
        # Remove ANSI escape sequences
        clean_line = re.sub(r'\x1b\[[0-9;]*m', '', line)
        
        # Extract timestamp, block number, and processing time
        timestamp_match = re.match(r'^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z)', clean_line)
        block_num = extract_block_number(clean_line)
        processing_time = extract_processing_time(clean_line)
        
        if timestamp_match and block_num is not None:
            timestamp = parse_timestamp(timestamp_match.group(1))
            if timestamp:
                batcher_ends.append((timestamp, block_num))
                if processing_time is not None:
                    processing_times_direct.append(processing_time)

print(f"Parsed {len(batcher_ends)} batcher end entries")
print(f"Extracted {len(processing_times_direct)} direct processing times")

if len(batcher_starts) < 10 or len(batcher_ends) < 10:
    print("Need at least 10 entries of each type for meaningful analysis")
    exit()

# Sort by block number
batcher_starts.sort(key=lambda x: x[1])
batcher_ends.sort(key=lambda x: x[1])

print(f"Batcher Start range: blocks {batcher_starts[0][1]} → {batcher_starts[-1][1]}")
print(f"Batcher End range: blocks {batcher_ends[0][1]} → {batcher_ends[-1][1]}")

# Method 1: Direct from log messages (most accurate)
if processing_times_direct:
    avg_time_direct = statistics.mean(processing_times_direct)
    median_time_direct = statistics.median(processing_times_direct)
    min_time_direct = min(processing_times_direct)
    max_time_direct = max(processing_times_direct)
    
    print(f"\nBATCHER PROCESSING TIME ANALYSIS (Direct from logs):")
    print(f"=" * 65)
    print(f"   Processing times analyzed: {len(processing_times_direct)}")
    print(f"   Average processing time: {avg_time_direct:.1f}ms")
    print(f"   Median processing time: {median_time_direct:.1f}ms")
    print(f"   Min processing time: {min_time_direct:.1f}ms")
    print(f"   Max processing time: {max_time_direct:.1f}ms")
    
    # Show distribution
    print(f"\nPROCESSING TIME DISTRIBUTION:")
    print(f"=" * 65)
    ranges = [
        (0, 5, "Very Fast"),
        (5, 10, "Fast"), 
        (10, 20, "Normal"),
        (20, 50, "Slow"),
        (50, float('inf'), "Very Slow")
    ]
    
    for min_val, max_val, label in ranges:
        count = len([t for t in processing_times_direct if min_val <= t < max_val])
        if count > 0:
            pct = count / len(processing_times_direct) * 100
            print(f"   {label:10} ({min_val:2.0f}-{max_val if max_val != float('inf') else '∞':>3}ms): {count:4d} blocks ({pct:4.1f}%)")
    
    # Show some examples
    print(f"\nEXAMPLE PROCESSING TIMES (first 10):")
    print(f"=" * 65)
    for i in range(min(10, len(processing_times_direct))):
        block_num = batcher_ends[i][1] if i < len(batcher_ends) else i
        time_ms = processing_times_direct[i]
        print(f"   Block {block_num:6d}: {time_ms:6.1f}ms")

# Method 2: Calculate from start/end timestamps (verification)
print(f"\nVERIFICATION: Calculating from start/end timestamps...")
matched_times = []
for start_time, start_block in batcher_starts:
    for end_time, end_block in batcher_ends:
        if end_block == start_block and end_time > start_time:
            time_diff_ms = (end_time - start_time).total_seconds() * 1000
            matched_times.append(time_diff_ms)
            break

if matched_times:
    avg_matched = statistics.mean(matched_times)
    print(f"   Matched pairs: {len(matched_times)}")
    print(f"   Average from timestamps: {avg_matched:.1f}ms")
    
    if processing_times_direct:
        diff = abs(avg_time_direct - avg_matched)
        print(f"   Difference: {diff:.1f}ms")
        if diff < 1.0:
            print("   Timestamps match log values (good!)")
        else:
            print("   Significant difference - using direct log values")

print(f"\nBATCHER COMPONENT SUMMARY:")
print(f"=" * 65)
if processing_times_direct:
    print(f"Batcher processes blocks in {avg_time_direct:.1f}ms on average")
    print(f"(from BATCHER_TIMING_START to BATCHER_TIMING_END)")
    print(f"\nThis represents the time for Batcher to:")
    print(f"• Receive the sync block from orchestrator")
    print(f"• Commit proposal and state diff to storage")
    print(f"• Notify L1 provider and mempool")
    print(f"• Complete all batcher operations")
    
    print(f"\nCOMPARISON WITH PREVIOUS ANALYSIS:")
    print(f"=" * 65)
    print(f"   Previous (wrong): 1ms (same-millisecond timestamps)")
    print(f"   Correct measurement: {avg_time_direct:.1f}ms")
    print(f"   The {avg_time_direct:.0f}x difference shows why detailed timing logs are crucial!")

print(f"\nLOCAL BATCHER TIMING ANALYSIS COMPLETE!")
EOF

# Cleanup
rm -f batcher_starts_local.txt batcher_ends_local.txt

echo ""
echo "NOTE ABOUT GOOGLE CLOUD LOGS:"
echo "=================================="
echo "Google Cloud logs don't include DEBUG level logs, so they're missing"
echo "BATCHER_TIMING_START logs. This caused the incorrect 1ms measurement."
echo "The local logs have the complete timing information needed for accurate analysis."
