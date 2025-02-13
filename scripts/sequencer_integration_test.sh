#!/usr/bin/env bash
# scripts/sequencer_integration_test.sh
#
# Usage:
#   ./scripts/sequencer_integration_test.sh [test]
#
# If no argument is provided, the default test "positive" is run.
# You can also pass:
#   - "positive" to run the positive flow test,
#   - "revert" to run the revert flow test,
#   - "restart" to run the restart flow test, or
#   - "all" to run all tests.
#
# Note: Make sure the binaries exist in
#   crates/starknet_integration_tests/src/bin/sequencer_node_integration_tests/
# with names such as positive_flow.rs, revert_flow.rs, restart_flow.rs.

# TODO(noamsp): find a way to get this mapping automatically instead of hardcoding
declare -A TEST_ALIASES=(
  [positive]="positive_flow_integration_test"
  [revert]="revert_flow_integration_test"
  [restart]="restart_flow_integration_test"
)

# Set default test if none provided
TEST="${1:-positive}"

echo "Running integration test alias: $TEST"

# Stop any running instances of starknet_sequencer_node (ignore error if not found)
killall starknet_sequencer_node 2>/dev/null

# Build the main node binary (if required)
cargo build --bin starknet_sequencer_node

# Helper function to run a test binary
run_test() {
  local tname="$1"
  echo "==> Running test: $tname"
  cargo run --bin "$tname" || { echo "Test $tname failed"; exit 1; }
}

if [ "$TEST" = "all" ]; then
  for alias in "" "${!TEST_ALIASES[@]}"; do
    run_test "${TEST_ALIASES[$alias]}"
  done
  exit 0
fi

if [ -z "${TEST_ALIASES[$TEST]}" ]; then
  echo "Invalid alias: '$TEST'"
  echo "Valid aliases are: all,$(IFS=,; echo "${!TEST_ALIASES[*]}")"
  exit 1
fi

run_test "${TEST_ALIASES[$TEST]}"