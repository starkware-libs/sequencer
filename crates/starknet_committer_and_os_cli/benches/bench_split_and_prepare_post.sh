#!/bin/env bash
#
# This script runs benchmarks, compares them to the baseline, and prepares results for posting.
# It will mark the CI to fail if any benchmark regresses by more than the threshold.
#
# Usage: bench_split_and_prepare_post.sh <benchmarks_list> <benchmark_results>
#   benchmarks_list: File containing list of benchmark names (one per line)
#   benchmark_results: Output file for formatted results (used for PR comments)

set -e

benchmarks_list=${1}
benchmark_results=${2}
# 8% threshold for acceptable regression.
threshold=8.0 

# ============================================================================
# Step 1: Run benchmarks.
# ============================================================================
# Run each benchmark individually and save output to separate files.
# TODO(Aner): split the output file instead.
cat ${benchmarks_list} |
    while read line; do
        cargo bench -p starknet_committer_and_os_cli $line > ${line}.txt;
        # Keep only the benchmark output, remove everything before the benchmark name.
        sed -i '/'"${line}"'/,$!d' ${line}.txt;
    done

# ============================================================================
# Step 2: Analyze results and check thresholds
# ============================================================================
echo "Benchmark movements: " > ${benchmark_results}
has_major_regression=false

cat ${benchmarks_list} |
    while read line; do
        # Parse the percentage change and check against threshold.
        # Find the line that contains the percent change summary printed by Criterion.
        # Typical format (note the leading spaces):
        #   "                        change: [-2.3% -1.2% +0.1%]"
        # Semantics: [lower_bound point_estimate upper_bound]
        change_line=$(grep "change:" ${line}.txt || true)
        if [ -n "$change_line" ]; then
            # Steps to extract the point estimate (the SECOND number inside the brackets):
            # 1) Split the line by whitespace and take field 3 using awk
            #    Fields: $1="change:"  $2="[-2.3%"  $3="-1.2%"  $4="+0.1%]"
            # 2) Strip '%' and any leading '+' so it's a clean float for bc
            #    Examples: "+6.4%" -> "6.4"; "-2.0%" -> "-2.0"
            change_pct=$(echo "$change_line" | awk '{print $3}' | tr -d '%+')
            if [ -n "$change_pct" ]; then
            
                # fail if point estimate > threshold.
                if awk -v x="$change_pct" -v y="$threshold" 'BEGIN { exit(!(x > y)) }'; then
                    if [ "$has_major_regression" = false ]; then
                        echo "" >> ${benchmark_results}
                        echo "---" >> ${benchmark_results}
                        echo "❌ **CI WILL FAIL: Benchmarks exceeded ${threshold}% regression threshold**" >> ${benchmark_results}
                        echo "" >> ${benchmark_results}
                        has_major_regression=true
                    fi
                    echo "ERROR: ${line} regressed by ${change_pct}%, exceeding ${threshold}% threshold!" >> ${benchmark_results}
                fi
            fi
        fi

        # Check if this benchmark regressed.
        if grep -q "regressed" ${line}.txt; then
            echo "**${line} performance regressed!**" >> ${benchmark_results};
            cat ${line}.txt >> ${benchmark_results};
            
        # Check if this benchmark improved.
        elif grep -q "improved" ${line}.txt; then
            echo "_${line} performance improved_ :smiley_cat:" >> ${benchmark_results};
            cat ${line}.txt >> ${benchmark_results};
        fi;
    done

# If no significant changes were detected.
if ! (grep -q "regressed" ${benchmark_results} || grep -q "improved" ${benchmark_results}); then
    echo "No major performance changes detected." >> ${benchmark_results};
fi
