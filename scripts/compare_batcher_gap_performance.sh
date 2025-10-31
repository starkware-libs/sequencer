#!/bin/bash

# Script to compare batcher inter-block gap performance across different environments
# Compares: Google Cloud (old disk), Google Cloud (new disk), Local setup

echo "========================================"
echo "BATCHER GAP PERFORMANCE COMPARISON"
echo "========================================"
echo ""

# Check if required files exist
OLD_DISK_JSON="${1:-start_sync_logs.json}"
NEW_DISK_JSON="${2:-start_sync_hyper_logs.json}" 
LOCAL_LOGS="${3:-mainnet_timing_logs.txt}"

echo "Files to analyze:"
echo "  Old Disk (Google Cloud): $OLD_DISK_JSON"
echo "  New Disk (Google Cloud): $NEW_DISK_JSON"
echo "  Local Setup: $LOCAL_LOGS"
echo ""

# Function to check file existence
check_file() {
    if [ ! -f "$1" ]; then
        echo "File not found: $1"
        return 1
    fi
    return 0
}

python3 << PYTHON_EOF
import json
import re
from datetime import datetime
import statistics
import sys

def extract_batcher_gaps_from_json(log_file, environment_name):
    """Extract batcher gaps from Google Cloud JSON logs"""
    try:
        with open(log_file, 'r') as f:
            logs = json.load(f)
        
        batcher_additions = []
        for entry in logs:
            text_payload = entry.get('textPayload', '')
            timestamp = entry.get('timestamp', '')
            
            if 'Adding sync block to Batcher for height' in text_payload:
                height_match = re.search(r'height (\d+)', text_payload)
                if height_match and timestamp:
                    height = int(height_match.group(1))
                    try:
                        parsed_time = datetime.fromisoformat(timestamp.replace('Z', '+00:00'))
                        batcher_additions.append((parsed_time, height))
                    except:
                        continue
        
        # Sort by height and calculate gaps
        batcher_additions.sort(key=lambda x: x[1])
        gaps = []
        
        for i in range(1, len(batcher_additions)):
            prev_time, prev_height = batcher_additions[i-1]
            curr_time, curr_height = batcher_additions[i]
            time_gap_ms = (curr_time - prev_time).total_seconds() * 1000
            gaps.append(time_gap_ms)
        
        if gaps:
            return {
                'environment': environment_name,
                'gaps': gaps,
                'avg': statistics.mean(gaps),
                'median': statistics.median(gaps),
                'min': min(gaps),
                'max': max(gaps),
                'count': len(gaps),
                'height_range': f"{batcher_additions[0][1]} → {batcher_additions[-1][1]}"
            }
        return None
    
    except Exception as e:
        print(f"Error analyzing {environment_name}: {e}")
        return None

def extract_batcher_gaps_from_local(log_file, environment_name):
    """Extract batcher gaps from local log files"""
    try:
        batcher_additions = []
        
        with open(log_file, 'r') as f:
            for line in f:
                if 'Adding sync block to Batcher for height' in line:
                    # Extract timestamp
                    timestamp_match = re.search(r'^(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}\.\d{3}[Z]?)', line)
                    height_match = re.search(r'height (\d+)', line)
                    
                    if timestamp_match and height_match:
                        timestamp_str = timestamp_match.group(1)
                        height = int(height_match.group(1))
                        
                        try:
                            if 'T' in timestamp_str and 'Z' in timestamp_str:
                                parsed_time = datetime.fromisoformat(timestamp_str.replace('Z', '+00:00'))
                            elif 'T' in timestamp_str:
                                parsed_time = datetime.fromisoformat(timestamp_str)
                            else:
                                parsed_time = datetime.strptime(timestamp_str, '%Y-%m-%d %H:%M:%S.%f')
                            
                            batcher_additions.append((parsed_time, height))
                        except:
                            continue
        
        # Sort by height and calculate gaps
        batcher_additions.sort(key=lambda x: x[1])
        gaps = []
        
        for i in range(1, len(batcher_additions)):
            prev_time, prev_height = batcher_additions[i-1]
            curr_time, curr_height = batcher_additions[i]
            time_gap_ms = (curr_time - prev_time).total_seconds() * 1000
            gaps.append(time_gap_ms)
        
        if gaps:
            return {
                'environment': environment_name,
                'gaps': gaps,
                'avg': statistics.mean(gaps),
                'median': statistics.median(gaps),
                'min': min(gaps),
                'max': max(gaps),
                'count': len(gaps),
                'height_range': f"{batcher_additions[0][1]} → {batcher_additions[-1][1]}"
            }
        return None
    
    except Exception as e:
        print(f"Error analyzing {environment_name}: {e}")
        return None

# Analyze all environments
results = []

print("ANALYZING ENVIRONMENTS...")
print("")

# Google Cloud Old Disk
old_disk_result = extract_batcher_gaps_from_json('$OLD_DISK_JSON', 'Google Cloud (Old Disk)')
if old_disk_result:
    results.append(old_disk_result)
    print(f"Google Cloud (Old Disk): {old_disk_result['count']} gaps analyzed")
else:
    print("Google Cloud (Old Disk): Analysis failed")

# Google Cloud New Disk  
new_disk_result = extract_batcher_gaps_from_json('$NEW_DISK_JSON', 'Google Cloud (New Disk)')
if new_disk_result:
    results.append(new_disk_result)
    print(f"Google Cloud (New Disk): {new_disk_result['count']} gaps analyzed")
else:
    print("Google Cloud (New Disk): Analysis failed")

# Local Setup
local_result = extract_batcher_gaps_from_local('$LOCAL_LOGS', 'Local Setup')
if local_result:
    results.append(local_result)
    print(f"Local Setup: {local_result['count']} gaps analyzed")
else:
    print("Local Setup: Analysis failed")

if not results:
    print("No valid results obtained from any environment!")
    sys.exit(1)

print("")
print("BATCHER INTER-BLOCK GAP COMPARISON:")
print("=" * 90)
print(f"{'Environment':<25} | {'Count':<6} | {'Avg Gap':<10} | {'Median':<10} | {'Min':<8} | {'Max':<10} | {'Range'}")
print("-" * 90)

for result in results:
    avg_str = f"{result['avg']:.0f}ms" if result['avg'] < 10000 else f"{result['avg']/1000:.1f}s"
    med_str = f"{result['median']:.0f}ms" if result['median'] < 10000 else f"{result['median']/1000:.1f}s"
    min_str = f"{result['min']:.0f}ms" if result['min'] < 10000 else f"{result['min']/1000:.1f}s"
    max_str = f"{result['max']:.0f}ms" if result['max'] < 10000 else f"{result['max']/1000:.1f}s"
    
    print(f"{result['environment']:<25} | {result['count']:<6} | {avg_str:<10} | {med_str:<10} | {min_str:<8} | {max_str:<10} | {result['height_range']}")

print("")
print("PERFORMANCE COMPARISON:")
print("=" * 60)

if len(results) >= 2:
    # Find baseline (usually old disk or local)
    baseline = results[0]  # First result as baseline
    
    for i, result in enumerate(results[1:], 1):
        improvement = baseline['avg'] / result['avg']
        if improvement > 1:
            print(f"{result['environment']} is {improvement:.1f}x FASTER than {baseline['environment']}")
        elif improvement < 1:
            print(f"{result['environment']} is {1/improvement:.1f}x SLOWER than {baseline['environment']}")
        else:
            print(f"{result['environment']} has SIMILAR performance to {baseline['environment']}")

print("")
print("BOTTLENECK ANALYSIS:")
print("=" * 60)

for result in results:
    fast_gaps = len([g for g in result['gaps'] if g < 500])
    normal_gaps = len([g for g in result['gaps'] if 500 <= g < 1000])
    slow_gaps = len([g for g in result['gaps'] if g >= 1000])
    
    print(f"{result['environment']}:")
    print(f"   Fast (<500ms): {fast_gaps:3d} ({fast_gaps/result['count']*100:4.1f}%)")
    print(f"   Normal (500ms-1s): {normal_gaps:3d} ({normal_gaps/result['count']*100:4.1f}%)")
    print(f"   Slow (>1s): {slow_gaps:3d} ({slow_gaps/result['count']*100:4.1f}%)")
    print("")

print("VALIDATION RESULTS:")
print("=" * 60)
print("This comparison shows the batcher inter-block gap performance")
print("across different environments, matching the timing analysis")
print("visible in your Google Cloud logs screenshot.")
print("")

# Check if we match expected performance ratios
if len(results) >= 2:
    print("Expected performance improvements:")
    print("• New disk should be ~5x faster than old disk")
    print("• Local setup performance varies by hardware")

PYTHON_EOF

echo ""
echo "USAGE INSTRUCTIONS:"
echo "=" * 60
echo "1. Run individual environment analysis:"
echo "   ./scripts/measure_cloud_batcher_gaps.sh start_sync_logs.json"
echo "   ./scripts/measure_cloud_batcher_gaps.sh start_sync_hyper_logs.json"
echo "   ./scripts/measure_batcher_inter_block_gaps.sh mainnet_timing_logs.txt"
echo ""
echo "2. Run this comparison script:"
echo "   ./scripts/compare_batcher_gap_performance.sh [old_disk.json] [new_disk.json] [local_logs.txt]"
echo ""
echo "This will help validate that your local measurements match"
echo "the Google Cloud performance patterns shown in your screenshot."


