#!/usr/bin/env bash
# scripts/sequencer_integration_test.sh
#
# Usage:
#   ./scripts/sequencer_integration_test.sh [flow]
#
# Runs one end-to-end sequencer integration-test flow via the integration_test_runner binary,
# or all flows in sequence with "all". Defaults to "positive". An unknown flow name lists the
# valid flows (including "all") and exits without building.

# Keep in sync with the Flow enum in
# crates/apollo_integration_tests/src/bin/integration_test_runner.rs.
FLOWS=(positive proof restart restart_multiple_nodes restart_single_node revert sync)

SEQUENCER_BINARY="apollo_node"
RUNNER_BINARY="integration_test_runner"

FLOW="${1:-positive}"

# Reject an unknown flow before the (minutes-long) node build, so bad input fails fast.
if [ "$FLOW" != "all" ] && [[ ! " ${FLOWS[*]} " == *" $FLOW "* ]]; then
  echo "Unknown flow: '$FLOW'"
  echo "Valid flows: all ${FLOWS[*]}"
  exit 1
fi

build_binary() {
  local binary_name="$1"
  echo "==> Building: $binary_name"
  cargo build --bin "$binary_name" || { echo "Build for $binary_name failed"; exit 1; }
}

run_flow() {
  local flow_name="$1"
  echo "==> Running flow: $flow_name"
  "./target/debug/$RUNNER_BINARY" "$flow_name" || { echo "Flow $flow_name failed"; exit 1; }
}

build_binary "$SEQUENCER_BINARY"
build_binary "$RUNNER_BINARY"

if [ "$FLOW" = "all" ]; then
  for flow_name in "${FLOWS[@]}"; do
    run_flow "$flow_name"
  done
  exit 0
fi

run_flow "$FLOW"
