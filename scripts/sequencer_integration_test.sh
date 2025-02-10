#!/usr/bin/env bash
# scripts/sequencer_integration_test.sh
#
# Usage:
#   ./scripts/sequencer_integration_test.sh [test]
#
# If no argument is provided, the default test "positive_flow" is run.
# You can also pass:
#   - "revert" to run the revert flow test,
#   - "reset" to run the reset flow test, or
#   - "all" to run all tests.
#
# Note: Make sure the binaries exist in
#   crates/starknet_integration_tests/src/bin/sequencer_node_integration_tests/
# with names such as positive_flow.rs, revert_flow.rs, reset_flow.rs.

# Set default test if none provided
TEST="${1:-positive_flow_integration_test}"

echo "Running integration test: $TEST"

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
  # Run all tests sequentially
  run_test "positive_flow_integration_test"
  run_test "revert_flow_integration_test"
  run_test "reset_flow_integration_test"
else
  # Map shorthand names if desired (e.g. "revert" -> "revert_flow")
  case "$TEST" in
    revert)
      TEST="revert_flow_integration_test"
      ;;
    reset)
      TEST="reset_flow_integration_test"
      ;;
    # if "positive_flow" or any other name is passed, we assume it matches the binary name
  esac

  run_test "$TEST"
fi
