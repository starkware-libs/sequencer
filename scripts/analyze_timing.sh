#!/bin/bash

# Simple timing analysis script
echo "Apollo Node Timing Analyzer"
echo "=============================="

if [ "$1" ]; then
    echo "Analyzing saved logs: $1"
    LOG_SOURCE="cat $1"
else
    echo "   Usage: $0 <log_file>"
    echo "   First save logs: RUST_LOG=debug ./scripts/run_sepolia_node.sh 2>&1 | tee timing_logs.txt"
    echo "   Then analyze: $0 timing_logs.txt"
    echo ""
    echo "   Or run live analysis (will show basic stats):"
    LOG_SOURCE="timeout 30s bash -c 'RUST_LOG=debug ./scripts/run_sepolia_node.sh 2>&1 | grep -E \"(COMMIT_TIMING|BLOCK_TIMING_COMPLETE)\"'"
fi

echo ""
echo "Extracting timing data..."

# Extract commit and total times
eval "$LOG_SOURCE" | grep -E "(COMMIT_TIMING|BLOCK_TIMING_COMPLETE)" | \
awk '
BEGIN {
    commit_count = 0
    total_count = 0
    commit_sum = 0
    total_sum = 0
}

/COMMIT_TIMING/ {
    if (match($0, /([0-9.]+)(ms|µs)/)) {
        value = substr($0, RSTART, RLENGTH-2)
        unit = substr($0, RSTART+length(value), 2)
        if (unit == "µs") value = value / 1000
        commit_times[commit_count] = value
        commit_sum += value
        commit_count++
    }
}

/BLOCK_TIMING_COMPLETE/ {
    if (match($0, /([0-9.]+)(ms|µs)/)) {
        value = substr($0, RSTART, RLENGTH-2)
        unit = substr($0, RSTART+length(value), 2)
        if (unit == "µs") value = value / 1000
        total_times[total_count] = value
        total_sum += value
        total_count++
    }
}

END {
    if (commit_count > 0) {
        avg_commit = commit_sum / commit_count
        avg_total = total_sum / total_count
        
        print ""
        printf "   Blocks analyzed: %d\n", commit_count
        printf "   Average commit/flush: %.2fms\n", avg_commit
        printf "   Average total time: %.2fms\n", avg_total
        printf "   Flush percentage: %.1f%%\n", (avg_commit/avg_total)*100
        printf "   Current throughput: ~%.0f blocks/sec\n", 1000/avg_total
        
        if (avg_commit > 0.5) {
            optimized_commit = avg_commit * 0.3
            optimized_total = avg_total - avg_commit + optimized_commit
            improvement = ((1000/optimized_total) - (1000/avg_total)) / (1000/avg_total) * 100
            printf "   With concurrent flush: ~%.0f blocks/sec\n", 1000/optimized_total
            printf "   Potential improvement: %.1f%% faster\n", improvement
        }
    } else {
        print "No timing data found. Make sure the node is running with RUST_LOG=debug"
    }
}'

echo ""
echo "🔍 SPECIFIC BLOCK ANALYSIS (Blocks 4251 & 4252 Comparison):"
echo "============================================================"

# Extract specific block data for comparison
eval "$LOG_SOURCE" | grep -E "(BLOCK_TIMING|STORAGE_TIMING|COMMIT_TIMING)" | awk '
BEGIN {
    current_block = 0
    blocks[4251]["found"] = 0
    blocks[4252]["found"] = 0
}

# Extract block number from any timing line
{
    if (match($0, /[Bb]lock ([0-9]+)/)) {
        current_block = substr($0, RSTART+6, RLENGTH-6)
    }
}

# Collect data for blocks 4251 and 4252
current_block == 4251 || current_block == 4252 {
    blocks[current_block]["found"] = 1
    
    if (/header write took/) {
        if (match($0, /([0-9.]+)(ms|µs)/)) {
            value = substr($0, RSTART, RLENGTH-2)
            unit = substr($0, RSTART+length(value), 2)
            if (unit == "µs") value = value / 1000
            blocks[current_block]["header"] = value
        }
    }
    else if (/signature write took/) {
        if (match($0, /([0-9.]+)(ms|µs)/)) {
            value = substr($0, RSTART, RLENGTH-2)
            unit = substr($0, RSTART+length(value), 2)
            if (unit == "µs") value = value / 1000
            blocks[current_block]["signature"] = value
        }
    }
    else if (/body write took/) {
        if (match($0, /([0-9.]+)(ms|µs)/)) {
            value = substr($0, RSTART, RLENGTH-2)
            unit = substr($0, RSTART+length(value), 2)
            if (unit == "µs") value = value / 1000
            blocks[current_block]["body"] = value
        }
    }
    else if (/commit \(includes flush\) took/) {
        if (match($0, /([0-9.]+)(ms|µs)/)) {
            value = substr($0, RSTART, RLENGTH-2)
            unit = substr($0, RSTART+length(value), 2)
            if (unit == "µs") value = value / 1000
            blocks[current_block]["commit"] = value
        }
    }
    else if (/total storage took/) {
        if (match($0, /([0-9.]+)(ms|µs)/)) {
            value = substr($0, RSTART, RLENGTH-2)
            unit = substr($0, RSTART+length(value), 2)
            if (unit == "µs") value = value / 1000
            blocks[current_block]["storage"] = value
        }
    }
    else if (/total processing took/) {
        if (match($0, /([0-9.]+)(ms|µs)/)) {
            value = substr($0, RSTART, RLENGTH-2)
            unit = substr($0, RSTART+length(value), 2)
            if (unit == "µs") value = value / 1000
            blocks[current_block]["total"] = value
        }
    }
}

END {
    # Display comparison with original results
    print ""
    print "BLOCK 4251 COMPARISON:"
    print "========================"
    if (blocks[4251]["found"]) {
        printf "   NEW RESULTS:\n"
        printf "   Header:      %.3f ms\n", blocks[4251]["header"]
        printf "   Signature:   %.3f ms\n", blocks[4251]["signature"] 
        printf "   Body:        %.3f ms\n", blocks[4251]["body"]
        printf "   Commit/Flush: %.3f ms\n", blocks[4251]["commit"]
        printf "   Total:       %.3f ms\n", blocks[4251]["total"]
        if (blocks[4251]["commit"] > 0 && blocks[4251]["storage"] > 0) {
            printf "   Flush %%:     %.1f%% of storage time\n", (blocks[4251]["commit"]/blocks[4251]["storage"])*100
        }
        
        print ""
        printf "ORIGINAL RESULTS (for comparison):\n"
        printf "   Header:      8.659 ms\n"
        printf "   Signature:   0.029 ms\n"
        printf "   Body:        0.506 ms\n"
        printf "   Commit/Flush: 2.236 ms\n"
        printf "   Total:       11.68 ms\n"
        printf "   Flush %%:     75%% of storage time\n"
        
        print ""
        if (blocks[4251]["header"] > 5.0) {
            print "   ANALYSIS: Similar HIGH header write time detected!"
        } else {
            print "   ANALYSIS: Different pattern - header write is normal this time"
        }
    } else {
        print "   Block 4251 not found in current logs"
    }
    
    print ""
    print "BLOCK 4252 COMPARISON:"
    print "========================="
    if (blocks[4252]["found"]) {
        printf "   NEW RESULTS:\n"
        printf "   Header:      %.3f ms\n", blocks[4252]["header"]
        printf "   Signature:   %.3f ms\n", blocks[4252]["signature"]
        printf "   Body:        %.3f ms\n", blocks[4252]["body"] 
        printf "   Commit/Flush: %.3f ms\n", blocks[4252]["commit"]
        printf "   Total:       %.3f ms\n", blocks[4252]["total"]
        if (blocks[4252]["commit"] > 0 && blocks[4252]["storage"] > 0) {
            printf "   Flush %%:     %.1f%% of storage time\n", (blocks[4252]["commit"]/blocks[4252]["storage"])*100
        }
        
        print ""
        printf "   ORIGINAL RESULTS (for comparison):\n"
        printf "   Header:      0.175 ms\n"
        printf "   Signature:   0.010 ms\n"
        printf "   Body:        0.402 ms\n"
        printf "   Commit/Flush: 1.373 ms\n"
        printf "   Total:       2.12 ms\n"
        printf "   Flush %%:     66%% of storage time\n"
        
        print ""
        if (blocks[4252]["total"] > 1.5 && blocks[4252]["total"] < 3.0) {
            print "   ANALYSIS: Similar TYPICAL block timing pattern!"
        } else {
            print "   ANALYSIS: Different pattern - timing varies from original"
        }
    } else {
        print "   Block 4252 not found in current logs"
    }
    
    print ""
    print "COMPARISON SUMMARY:"
    if (blocks[4251]["found"] && blocks[4252]["found"]) {
        print "   Both blocks found - comparison complete!"
        print "   Look for similar patterns in header write spikes and commit times"
    } else {
        print "   One or both blocks missing - may need to run longer to reach these block numbers"
    }
}'

echo ""
echo "Analysis complete!"
echo ""
echo "   To get detailed breakdown, save logs and run:"
echo "   RUST_LOG=debug ./scripts/run_sepolia_node.sh 2>&1 | tee timing_logs.txt"
echo "   $0 timing_logs.txt"
