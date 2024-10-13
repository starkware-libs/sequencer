#!/bin/bash

# This script analyses the peformance changes of commit $2 with respect to $1.

cd "$(dirname "$0")"

BEFORE_CHANGES_COMMIT=$1
AFTER_CHANGES_COMMIT=$2

git checkout -q $BEFORE_CHANGES_COMMIT
echo "Running bechmark on commit $(cat ../.git/HEAD)"
cargo bench --quiet --locked --bench blockifier_bench -- --save-baseline before_changes --noplot > /dev/null 2>&1

git checkout -q $AFTER_CHANGES_COMMIT
echo "Running bechmark on commit $(cat ../.git/HEAD)"
cargo bench --quiet --locked --bench blockifier_bench -- --save-baseline after_changes --noplot > /dev/null 2>&1

cargo bench --quiet --bench blockifier_bench -- --load-baseline after_changes --baseline before_changes --noplot --verbose
