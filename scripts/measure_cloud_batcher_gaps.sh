#!/bin/bash

# Script to measure inter-block gaps for "Adding sync block to Batcher" logs from Google Cloud JSON
# This analyzes the same timing as shown in your Google Cloud screenshot

LOG_FILE="${1:-start_sync_logs.json}"
OUTPUT_MODE="${2:-detailed}"  # detailed, summary, or csv

echo "========================================"
echo "GOOGLE CLOUD BATCHER GAP ANALYSIS"
echo "========================================"
echo ""
echo "Analyzing: $LOG_FILE"
echo "Mode: $OUTPUT_MODE"
echo ""

if [ ! -f "$LOG_FILE" ]; then
    echo "Log file not found: $LOG_FILE"
    echo ""
    echo "Usage: $0 <google_cloud_json_file> [detailed|summary|csv]"
    echo "Example: $0 start_sync_logs.json detailed"
    exit 1
fi

python3 << PYTHON_EOF
import json
import re
from datetime import datetime
import statistics

def extract_batcher_additions_from_json(log_file):
    """Extract 'Adding sync block to Batcher' entries from Google Cloud JSON logs"""
    batcher_additions = []
    
    try:
        with open(log_file, 'r') as f:
            logs = json.load(f)
        
        for entry in logs:
            text_payload = entry.get('textPayload', '')
            timestamp = entry.get('timestamp', '')
            
            # Look for "Adding sync block to Batcher for height" pattern
            if 'Adding sync block to Batcher for height' in text_payload:
                # Extract height
                height_match = re.search(r'height (\d+)', text_payload)
                
                if height_match and timestamp:
                    height = int(height_match.group(1))
                    
                    # Parse Google Cloud timestamp
                    try:
                        parsed_time = datetime.fromisoformat(timestamp.replace('Z', '+00:00'))
                        batcher_additions.append((parsed_time, height, text_payload))
                    except:
                        continue
        
        return sorted(batcher_additions, key=lambda x: x[1])  # Sort by height
    
    except Exception as e:
        print(f"Error reading JSON file: {e}")
        return []

def calculate_inter_block_gaps(batcher_additions):
    """Calculate time gaps between consecutive batcher additions"""
    gaps = []
    
    for i in range(1, len(batcher_additions)):
        prev_time, prev_height, prev_text = batcher_additions[i-1]
        curr_time, curr_height, curr_text = batcher_additions[i]
        
        # Calculate time gap in milliseconds
        time_gap_ms = (curr_time - prev_time).total_seconds() * 1000
        height_gap = curr_height - prev_height
        
        gaps.append({
            'prev_height': prev_height,
            'curr_height': curr_height,
            'height_gap': height_gap,
            'time_gap_ms': time_gap_ms,
            'prev_time': prev_time,
            'curr_time': curr_time
        })
    
    return gaps

# Main analysis
try:
    print("EXTRACTING GOOGLE CLOUD BATCHER LOGS...")
    batcher_additions = extract_batcher_additions_from_json('$LOG_FILE')
    
    if not batcher_additions:
        print("No 'Adding sync block to Batcher for height' logs found in JSON!")
        print("")
        print("Expected JSON structure:")
        print('   "textPayload": "...Adding sync block to Batcher for height XXXXX..."')
        print("")
        print("Make sure your Google Cloud JSON contains these log messages.")
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
    
    print("GOOGLE CLOUD INTER-BLOCK GAP STATISTICS:")
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
        print("GOOGLE CLOUD SUMMARY:")
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
        print("DETAILED GOOGLE CLOUD INTER-BLOCK GAPS:")
        print("=" * 80)
        print(f"{'From':>6} | {'To':>6} | {'Gap':>8} | {'Time From':>12} | {'Time To':>12}")
        print("-" * 80)
        
        # Show all gaps (or recent ones if too many)
        display_gaps = gaps[-25:] if len(gaps) > 25 else gaps
        
        for gap in display_gaps:
            gap_str = f"{gap['time_gap_ms']:.0f}ms" if gap['time_gap_ms'] < 10000 else f"{gap['time_gap_ms']/1000:.1f}s"
            
            prev_time_str = gap['prev_time'].strftime('%H:%M:%S.%f')[:-3]
            curr_time_str = gap['curr_time'].strftime('%H:%M:%S.%f')[:-3]
            
            print(f"{gap['prev_height']:6d} | {gap['curr_height']:6d} | {gap_str:>8} | {prev_time_str:>12} | {curr_time_str:>12}")
        
        if len(gaps) > 25:
            print(f"... and {len(gaps) - 25} more gaps")
    
    print("")
    print("GOOGLE CLOUD BOTTLENECK ANALYSIS:")
    print("=" * 60)
    
    # Identify bottlenecks based on your screenshot expectations
    fast_gaps = [g for g in gaps if g['time_gap_ms'] < 500]  # <500ms (good performance)
    normal_gaps = [g for g in gaps if 500 <= g['time_gap_ms'] < 1000]  # 500ms-1s
    slow_gaps = [g for g in gaps if g['time_gap_ms'] >= 1000]  # >1 second
    
    print(f"Fast gaps (<500ms): {len(fast_gaps)} ({len(fast_gaps)/len(gaps)*100:.1f}%)")
    print(f"Normal gaps (500ms-1s): {len(normal_gaps)} ({len(normal_gaps)/len(gaps)*100:.1f}%)")
    print(f"Slow gaps (>1s): {len(slow_gaps)} ({len(slow_gaps)/len(gaps)*100:.1f}%)")
    
    if slow_gaps:
        print("")
        print("   Slowest gaps:")
        worst_gaps = sorted(slow_gaps, key=lambda x: x['time_gap_ms'], reverse=True)[:5]
        for gap in worst_gaps:
            print(f"      Height {gap['prev_height']} → {gap['curr_height']}: {gap['time_gap_ms']:.0f}ms")
    
    print("")
    print("COMPARISON BASELINE:")
    print("=" * 60)
    print("This Google Cloud measurement provides the baseline for comparison")
    print("with local measurements and different disk configurations.")
    print("")
    print(f"Key metric: {avg_gap:.0f}ms average inter-block gap")

except Exception as e:
    print(f"Error analyzing Google Cloud batcher gaps: {e}")
    import traceback
    traceback.print_exc()
PYTHON_EOF
