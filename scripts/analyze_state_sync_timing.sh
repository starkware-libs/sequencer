#!/bin/bash

# Script to analyze State Sync processing timing from Google Cloud logs
# Measures: State Sync Start → State Sync Storage Write

set -e

LOG_FILE="${1:-/home/dean/Documents/logs/regular500k.json}"
MODE="${2:-detailed}"

echo "========================================"
echo "STATE SYNC TIMING ANALYSIS"
echo "========================================"
echo ""
echo "Analyzing: $LOG_FILE"
echo "Mode: $MODE"
echo ""

if [ ! -f "$LOG_FILE" ]; then
    echo "Log file not found: $LOG_FILE"
    exit 1
fi

echo "EXTRACTING STATE SYNC TIMING LOGS..."

# Extract state sync start logs
echo "Looking for State Sync start logs..."
grep 'Storing state diff' "$LOG_FILE" > state_sync_starts.txt 2>/dev/null || {
    echo "No 'Storing state diff' logs found!"
    echo ""
    echo "Expected log pattern:"
    echo "   'Storing state diff. block_number=BlockNumber(XXXXX)'"
    echo ""
    echo "Let me check what state sync logs are available..."
    grep -i "state.*sync" "$LOG_FILE" | head -5
    exit 1
}

# Extract state sync storage completion logs  
echo "Looking for State Sync storage completion logs..."
grep 'SYNC_NEW_BLOCK: Added block' "$LOG_FILE" > state_sync_storage.txt 2>/dev/null || {
    echo "No 'SYNC_NEW_BLOCK: Added block' logs found!"
    echo ""
    echo "Expected log pattern:"
    echo "   'SYNC_NEW_BLOCK: Added block XXXXX'"
    echo ""
    echo "Let me check what sync completion logs are available..."
    grep -i "sync.*new.*block" "$LOG_FILE" | head -5
    exit 1
}

START_COUNT=$(wc -l < state_sync_starts.txt)
STORAGE_COUNT=$(wc -l < state_sync_storage.txt)

echo "Found $START_COUNT state sync start logs"
echo "Found $STORAGE_COUNT state sync storage logs"

if [ "$START_COUNT" -eq 0 ] || [ "$STORAGE_COUNT" -eq 0 ]; then
    echo "Need both start and storage logs to calculate timing"
    exit 1
fi

echo ""
echo "CALCULATING STATE SYNC PROCESSING TIMES..."

python3 << 'EOF'
import json
import re
from datetime import datetime
import statistics

def parse_google_cloud_timestamp(timestamp_str):
    """Parse Google Cloud timestamp format"""
    # Handle format: 2024-XX-XXTXX:XX:XX.XXXXXZ
    try:
        return datetime.fromisoformat(timestamp_str.replace('Z', '+00:00'))
    except:
        return None

def extract_timestamp_from_textpayload(text):
    """Extract timestamp from textPayload field"""
    # Handle format: "2025-08-25T20:50:32.116Z DEBUG ..."
    matches = re.match(r'^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z)', text)
    if matches:
        return parse_google_cloud_timestamp(matches.group(1))
    return None

def extract_block_number(text):
    """Extract block number from log message"""
    # Handle Google Cloud format: BlockNumber(511673)
    matches = re.findall(r'BlockNumber\((\d+)\)', text)
    if matches:
        return int(matches[0])
    # Handle other formats
    matches = re.findall(r'block (\d+)', text)
    if matches:
        return int(matches[0])
    matches = re.findall(r'Block (\d+)', text)
    if matches:
        return int(matches[0])
    matches = re.findall(r'height (\d+)', text)
    if matches:
        return int(matches[0])
    return None

# Parse state sync starts
print("Parsing state sync start logs...")
sync_starts = []

with open('state_sync_starts.txt', 'r') as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
            
        # Extract log message from textPayload field
        if line.startswith('"textPayload": "') and line.endswith('",'):
            message = line[16:-2]  # Remove '"textPayload": "' and '",'
            
            # Extract timestamp and block number from the log message
            timestamp = extract_timestamp_from_textpayload(message)
            block_num = extract_block_number(message)
            
            if timestamp and block_num is not None:
                sync_starts.append((timestamp, block_num))

print(f"Parsed {len(sync_starts)} state sync start entries")

# Parse state sync storage completions
print("Parsing state sync storage completion logs...")
storage_completions = []

with open('state_sync_storage.txt', 'r') as f:
    for line in f:
        line = line.strip()
        if not line:
            continue
            
        # Extract log message from textPayload field
        if line.startswith('"textPayload": "') and line.endswith('",'):
            message = line[16:-2]  # Remove '"textPayload": "' and '",'
            
            # Extract timestamp and block number from the log message
            timestamp = extract_timestamp_from_textpayload(message)
            block_num = extract_block_number(message)
            
            if timestamp and block_num is not None:
                storage_completions.append((timestamp, block_num))

print(f"Parsed {len(storage_completions)} storage completion entries")

if len(sync_starts) < 10 or len(storage_completions) < 10:
    print("Need at least 10 entries of each type for meaningful analysis")
    exit()

# Sort by block number
sync_starts.sort(key=lambda x: x[1])
storage_completions.sort(key=lambda x: x[1])

print(f"State Sync Start range: blocks {sync_starts[0][1]} → {sync_starts[-1][1]}")
print(f"Storage Completion range: blocks {storage_completions[0][1]} → {storage_completions[-1][1]}")

# Match starts with completions
processing_times = []
matched_blocks = []

for start_time, start_block in sync_starts:
    # Find corresponding storage completion
    for storage_time, storage_block in storage_completions:
        if storage_block == start_block and storage_time > start_time:
            processing_time_ms = (storage_time - start_time).total_seconds() * 1000
            processing_times.append(processing_time_ms)
            matched_blocks.append(start_block)
            break

if not processing_times:
    print("No matching start/completion pairs found")
    print("This might indicate different block numbering or timing issues")
    exit()

# Calculate statistics
avg_time = statistics.mean(processing_times)
median_time = statistics.median(processing_times)
min_time = min(processing_times)
max_time = max(processing_times)

print(f"\nSTATE SYNC PROCESSING TIME ANALYSIS:")
print(f"=" * 60)
print(f"   Matched blocks analyzed: {len(processing_times)}")
print(f"   Block range: {min(matched_blocks)} → {max(matched_blocks)}")
print(f"   Average processing time: {avg_time:.1f}ms")
print(f"   Median processing time: {median_time:.1f}ms")
print(f"   Min processing time: {min_time:.1f}ms")
print(f"   Max processing time: {max_time:.1f}ms")

# Show distribution
print(f"\nPROCESSING TIME DISTRIBUTION:")
print(f"=" * 60)
ranges = [
    (0, 100, "Very Fast"),
    (100, 500, "Fast"), 
    (500, 1000, "Normal"),
    (1000, 5000, "Slow"),
    (5000, float('inf'), "Very Slow")
]

for min_val, max_val, label in ranges:
    count = len([t for t in processing_times if min_val <= t < max_val])
    if count > 0:
        pct = count / len(processing_times) * 100
        print(f"   {label:10} ({min_val:4.0f}-{max_val if max_val != float('inf') else '∞':>5}ms): {count:3d} blocks ({pct:4.1f}%)")

# Show some examples
print(f"\nEXAMPLE PROCESSING TIMES (first 10 matched blocks):")
print(f"=" * 60)
for i in range(min(10, len(processing_times))):
    block = matched_blocks[i]
    time_ms = processing_times[i]
    print(f"   Block {block:6d}: {time_ms:6.1f}ms")

print(f"\nSTATE SYNC COMPONENT SUMMARY:")
print(f"=" * 60)
print(f"State Sync processes blocks in {avg_time:.0f}ms on average")
print(f"(from 'Starting block processing' to 'Storage commit completed')")
print(f"\nThis represents the time for State Sync to:")
print(f"• Receive and validate the block")
print(f"• Update the state tree")
print(f"• Commit changes to storage")
EOF

# Cleanup
rm -f state_sync_starts.txt state_sync_storage.txt

echo ""
echo "STATE SYNC TIMING ANALYSIS COMPLETE!"
