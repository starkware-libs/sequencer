#!/bin/env bash

set -e

benchmarks_list=${1}
benchmark_results=${2}
# Benchmark the new code, splitting the benchmarks
# TODO(Aner): split the output file instead.
cat ${benchmarks_list} |
    while read line; do
        cargo bench -p starknet_committer_and_os_cli $line > ${line}.txt;
        sed -i '/'"${line}"'/,$!d' ${line}.txt;
    done

# Prepare the results for posting comment.
echo "Benchmark movements: " > ${benchmark_results}
cat ${benchmarks_list} |
    while read line; do
        if grep -q "regressed" ${line}.txt; then
            echo "**${line} performance regressed!**" >> ${benchmark_results};
            cat ${line}.txt >> ${benchmark_results};
        elif grep -q "improved" ${line}.txt; then
            echo "_${line} performance improved_ :smiley_cat:" >> ${benchmark_results};
            cat ${line}.txt >> ${benchmark_results};
        fi;
    done
if ! (grep -q "regressed" ${benchmark_results} || grep -q "improved" ${benchmark_results}); then
    echo "No major performance changes detected." >> ${benchmark_results};
fi
