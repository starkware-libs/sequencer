#!/bin/bash

# Script to measure inter-block gaps for "Adding sync block to Batcher" logs
# This measures the time difference between adjacent batcher block additions

LOG_FILE="${1:-mainnet_timing_logs.txt}"
OUTPUT_MODE="${2:-detailed}"  # detailed, summary, or csv

echo "========================================"
echo "BATCHER INTER-BLOCK GAP ANALYSIS"
echo "========================================"
echo ""
echo "Analyzing: $LOG_FILE"
echo "Mode: $OUTPUT_MODE"
echo ""

if [ ! -f "$LOG_FILE" ]; then
    echo "Log file not found: $LOG_FILE"
    echo ""
    echo "Usage: $0 <log_file> [detailed|summary|csv]"
    echo "Example: $0 mainnet_timing_logs.txt detailed"
    exit 1
fi

python3 << PYTHON_EOF
import re
from datetime import datetime
import statistics

def parse_timestamp(timestamp_str):
    """Parse timestamp from log line"""
    try:
        # Handle different timestamp formats
        if 'T' in timestamp_str and 'Z' in timestamp_str:
            # ISO format: 2025-08-24T08:25:33.483Z
            return datetime.fromisoformat(timestamp_str.replace('Z', '+00:00'))
        elif 'T' in timestamp_str:
            # ISO format without Z: 2025-08-24T08:25:33.483
            return datetime.fromisoformat(timestamp_str)
        else:
            # Try other common formats
            return datetime.strptime(timestamp_str, '%Y-%m-%d %H:%M:%S.%f')
    except:
        return None

def extract_batcher_additions(log_file):
    """Extract 'Adding sync block to Batcher' entries with timestamps and heights"""
    batcher_additions = []
    
    with open(log_file, 'r') as f:
        for line_num, line in enumerate(f, 1):
            # Look for "Adding sync block to Batcher for height" pattern (with or without ORCHESTRATOR_TIMING_START)
            if 'Adding sync block to Batcher for height' in line:
                # Extract timestamp (first part of line)
                timestamp_match = re.search(r'^(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}\.\d{3}[Z]?)', line)
                
                # Extract height
                height_match = re.search(r'height (\d+)', line)
                
                if timestamp_match and height_match:
                    timestamp_str = timestamp_match.group(1)
                    height = int(height_match.group(1))
                    
                    parsed_time = parse_timestamp(timestamp_str)
                    if parsed_time:
                        batcher_additions.append((parsed_time, height, line_num, line.strip()))
    
    return sorted(batcher_additions, key=lambda x: x[1])  # Sort by height

def calculate_inter_block_gaps(batcher_additions):
    """Calculate time gaps between consecutive batcher additions"""
    gaps = []
    
    for i in range(1, len(batcher_additions)):
        prev_time, prev_height, prev_line, prev_text = batcher_additions[i-1]
        curr_time, curr_height, curr_line, curr_text = batcher_additions[i]
        
        # Calculate time gap in milliseconds
        time_gap_ms = (curr_time - prev_time).total_seconds() * 1000
        height_gap = curr_height - prev_height
        
        gaps.append({
            'prev_height': prev_height,
            'curr_height': curr_height,
            'height_gap': height_gap,
            'time_gap_ms': time_gap_ms,
            'prev_time': prev_time,
            'curr_time': curr_time,
            'prev_line': prev_line,
            'curr_line': curr_line
        })
    
    return gaps

# Main analysis
try:
    print("EXTRACTING BATCHER ADDITION LOGS...")
    batcher_additions = extract_batcher_additions('$LOG_FILE')
    
    if not batcher_additions:
        print("No 'Adding sync block to Batcher for height' logs found!")
        print("")
        print("Expected log pattern:")
        print("   'Adding sync block to Batcher for height XXXXX'")
        print("")
        print("Make sure your log file contains these specific log messages.")
        exit(1)
    
    print(f"Found {len(batcher_additions)} batcher addition logs")
    print(f"   Height range: {batcher_additions[0][1]} → {batcher_additions[-1][1]}")
    print("")
    
    print("CALCULATING INTER-BLOCK GAPS...")
    gaps = calculate_inter_block_gaps(batcher_additions)
    
    if not gaps:
        print("Need at least 2 batcher additions to calculate gaps!")
        exit(1)
    
    print(f"Calculated {len(gaps)} inter-block gaps")
    print("")
    
    # Statistical analysis
    gap_times = [gap['time_gap_ms'] for gap in gaps]
    avg_gap = statistics.mean(gap_times)
    median_gap = statistics.median(gap_times)
    min_gap = min(gap_times)
    max_gap = max(gap_times)
    
    print("INTER-BLOCK GAP STATISTICS:")
    print("=" * 60)
    print(f"   Total gaps analyzed: {len(gaps)}")
    print(f"   Average gap: {avg_gap:.1f}ms")
    print(f"   Median gap: {median_gap:.1f}ms")
    print(f"   Min gap: {min_gap:.1f}ms")
    print(f"   Max gap: {max_gap:.1f}ms")
    print("")
    
    # Output based on mode
    output_mode = '$OUTPUT_MODE'
    
    if output_mode == 'csv':
        print("HEIGHT_FROM,HEIGHT_TO,GAP_MS,TIMESTAMP_FROM,TIMESTAMP_TO")
        for gap in gaps:
            print(f"{gap['prev_height']},{gap['curr_height']},{gap['time_gap_ms']:.1f},{gap['prev_time'].isoformat()},{gap['curr_time'].isoformat()}")
    
    elif output_mode == 'summary':
        print("SUMMARY RESULTS:")
        print("=" * 60)
        print(f"Average inter-block gap: {avg_gap:.0f}ms")
        if avg_gap > 1000:
            print(f"                       ({avg_gap/1000:.1f} seconds)")
        print("")
        
        # Show distribution
        ranges = [
            (0, 100, "Very Fast"),
            (100, 500, "Fast"), 
            (500, 1000, "Normal"),
            (1000, 5000, "Slow"),
            (5000, float('inf'), "Very Slow")
        ]
        
        for min_val, max_val, label in ranges:
            count = len([g for g in gap_times if min_val <= g < max_val])
            if count > 0:
                pct = count / len(gap_times) * 100
                print(f"   {label:10} ({min_val:4.0f}-{max_val if max_val != float('inf') else '∞':>4}ms): {count:3d} blocks ({pct:4.1f}%)")
    
    else:  # detailed mode
        print("DETAILED INTER-BLOCK GAPS:")
        print("=" * 80)
        print(f"{'From':>6} | {'To':>6} | {'Gap':>8} | {'Time From':>12} | {'Time To':>12}")
        print("-" * 80)
        
        # Show recent gaps (last 20)
        recent_gaps = gaps[-20:] if len(gaps) > 20 else gaps
        
        for gap in recent_gaps:
            gap_str = f"{gap['time_gap_ms']:.0f}ms" if gap['time_gap_ms'] < 10000 else f"{gap['time_gap_ms']/1000:.1f}s"
            
            prev_time_str = gap['prev_time'].strftime('%H:%M:%S.%f')[:-3]
            curr_time_str = gap['curr_time'].strftime('%H:%M:%S.%f')[:-3]
            
            print(f"{gap['prev_height']:6d} | {gap['curr_height']:6d} | {gap_str:>8} | {prev_time_str:>12} | {curr_time_str:>12}")
        
        if len(gaps) > 20:
            print(f"... and {len(gaps) - 20} more gaps")
    
    print("")
    print("BOTTLENECK ANALYSIS:")
    print("=" * 60)
    
    # Identify bottlenecks
    slow_gaps = [g for g in gaps if g['time_gap_ms'] > 1000]  # >1 second
    very_slow_gaps = [g for g in gaps if g['time_gap_ms'] > 5000]  # >5 seconds
    
    if very_slow_gaps:
        print(f"SEVERE BOTTLENECKS: {len(very_slow_gaps)} gaps >5 seconds")
        print("   Worst cases:")
        worst_gaps = sorted(very_slow_gaps, key=lambda x: x['time_gap_ms'], reverse=True)[:5]
        for gap in worst_gaps:
            print(f"      Height {gap['prev_height']} → {gap['curr_height']}: {gap['time_gap_ms']:.0f}ms")
    
    elif slow_gaps:
        print(f"BOTTLENECKS DETECTED: {len(slow_gaps)} gaps >1 second")
        print(f"   Frequency: {len(slow_gaps)/len(gaps)*100:.1f}% of blocks")
    
    else:
        print("NO MAJOR BOTTLENECKS: All gaps <1 second")
    
    print("")
    print("This measurement shows the time between consecutive")
    print("'Adding sync block to Batcher' operations, which reveals")
    print("the actual batcher processing pipeline bottlenecks.")

except Exception as e:
    print(f"Error analyzing batcher gaps: {e}")
    import traceback
    traceback.print_exc()
PYTHON_EOF
