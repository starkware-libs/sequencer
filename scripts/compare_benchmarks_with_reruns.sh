#!/bin/bash

# Compares the benchmarks of the PR checkout (the current directory) against the base checkout at
# GITHUB_WORKSPACE, up to 3 times. Exits 0 at the first passing comparison and 1 if all 3 fail.
#
# Usage: compare_benchmarks_with_reruns.sh <base results dir> <run args>... -- <comparison args>...
#   <run args> go to both the base reruns and the comparisons, e.g. --package and --input-dir.
#   <comparison args> go to `run-and-compare` only.

set -euo pipefail

readonly MAX_ATTEMPTS=3

function usage() {
    echo "Usage: $0 <base results dir> <run args>... -- <comparison args>..." >&2
    exit 2
}

[ "$#" -ge 1 ] || usage
base_results_dir="$1"
shift
run_args=()
while [ "$#" -gt 0 ] && [ "$1" != "--" ]; do
    run_args+=("$1")
    shift
done
[ "$#" -ge 1 ] || usage
shift

for ((attempt = 1; attempt <= MAX_ATTEMPTS; attempt++)); do
    if cargo run -p bench_tools -- run-and-compare "${run_args[@]}" "$@"; then
        exit 0
    fi
    if [ "${attempt}" -eq "${MAX_ATTEMPTS}" ]; then
        echo "::error::Benchmark comparison failed ${MAX_ATTEMPTS} of ${MAX_ATTEMPTS} times."
        exit 1
    fi
    echo "::warning::Benchmark comparison ${attempt} of ${MAX_ATTEMPTS} failed, rerunning the" \
        "base and PR benchmarks (attempt $((attempt + 1)) of ${MAX_ATTEMPTS})."
    # Criterion compares each run against <bench>/base and then saves the run there, so the PR
    # checkout's base/ now holds the PR measurement. Replace it with a fresh base measurement.
    (cd "${GITHUB_WORKSPACE}" &&
        cargo run -p bench_tools -- run "${run_args[@]}" --out "${base_results_dir}")
    rm -rf target/criterion
    cp -r "${GITHUB_WORKSPACE}/target/criterion" target/
done
