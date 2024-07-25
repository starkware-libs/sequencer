set -e

# Restore security level in perf_event_paranoid at the end.
ORIGINAL_PARANOID=$(echo $(sysctl kernel.perf_event_paranoid) | grep -o '[0-9]$')
trap 'sudo sysctl kernel.perf_event_paranoid=$ORIGINAL_PARANOID' EXIT SIGINT SIGTERM

if ! command -v jq; then
    cargo install jq
fi
if ! command -v flamegraph; then
    cargo install flamegraph
fi
if ! command -v perf; then
    sudo apt-get install linux-tools-common linux-tools-generic linux-tools-`uname -r`
fi

ROOT_DIR=$(git rev-parse --show-toplevel)
BENCH_INPUT_FILES_PREFIX=$(cat ${ROOT_DIR}/crates/committer_cli/src/tests/flow_test_files_prefix)
# Lower security level in perf_event_paranoid to 2 to allow cargo to use perf without running on root.
sudo sysctl kernel.perf_event_paranoid=2

gcloud storage cat gs://committer-testing-artifacts/${BENCH_INPUT_FILES_PREFIX}/committer_flow_inputs.json | jq -r .committer_input | CARGO_PROFILE_RELEASE_DEBUG=true cargo flamegraph -p committer_cli -- commit
