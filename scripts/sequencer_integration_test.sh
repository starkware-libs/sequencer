#!/usr/bin/env bash
# scripts/sequencer_integration_test.sh
#
# Usage:
#   ./scripts/sequencer_integration_test.sh [test]
#
# If no argument is provided, the default test "positive" is run.
# You can also pass:
#   - "positive" to run the positive flow test,
#   - "restart" to run the restart flow test, or
#   - "revert" to run the revert flow test,
#   - "sync" to run the central and p2p sync flow test,
#   - "all" to run all tests.
#
# Note: Make sure the binaries exist in
#   crates/apollo_integration_tests/src/bin/sequencer_node_integration_tests/
# with names such as positive_flow.rs, revert_flow.rs, restart_flow.rs, sync_flow.rs

# TODO(noamsp): find a way to get this mapping automatically instead of hardcoding
declare -A TEST_ALIASES=(
  [positive]="integration_test_positive_flow"
  [restart]="integration_test_restart_flow"
  [revert]="integration_test_revert_flow"
  [sync]="integration_test_central_and_p2p_sync_flow"
  [single_node]="run_single_node"
)

# Set default test if none provided
TEST="${1:-positive}"

echo "Running integration test alias: $TEST"

SEQUENCER_BINARY="apollo_node"

# Build the main node binary (if required)
cargo build --bin "$SEQUENCER_BINARY"

# Helper function to build a test binary
build_test() {
  local tname="$1"
  echo "==> Building test: $tname"
  cargo build --bin "$tname" || { echo "Build for $tname failed"; exit 1; }
}

# Helper function to run a test binary
run_test() {
  local tname="$1"
  echo "==> Running test: $tname"
  "./target/debug/$tname" || { echo "Test $tname failed"; exit 1; }
}

if [ "$TEST" = "all" ]; then
  for alias in "${!TEST_ALIASES[@]}"; do
    build_test "${TEST_ALIASES[$alias]}"
  done
  for alias in "${!TEST_ALIASES[@]}"; do
    run_test "${TEST_ALIASES[$alias]}"
  done
  exit 0
fi

if [ -z "${TEST_ALIASES[$TEST]}" ]; then
  echo "Invalid alias: '$TEST'"
  echo "Valid aliases are: all,$(IFS=,; echo "${!TEST_ALIASES[*]}")"
  exit 1
fi

build_test "${TEST_ALIASES[$TEST]}"
run_test "${TEST_ALIASES[$TEST]}"
