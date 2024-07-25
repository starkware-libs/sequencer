trap 'echo 4 | sudo tee /proc/sys/kernel/perf_event_paranoid ' EXIT SIGINT SIGTERM

if ! command -v jq; then
    cargo install jq
fi
if ! command -v flamegraph; then
    cargo install flamegraph
fi
if ! command -v perf; then
    sudo apt-get install linux-tools-common linux-tools-generic linux-tools-`uname -r`
fi

BENCH_INPUT_FILES_PREFIX=$(cat ./crates/committer_cli/src/tests/flow_test_files_prefix)
echo 2 | sudo tee /proc/sys/kernel/perf_event_paranoid

gcloud storage cat gs://committer-testing-artifacts/${BENCH_INPUT_FILES_PREFIX}/committer_flow_inputs.json | jq -r .committer_input | CARGO_PROFILE_RELEASE_DEBUG=true cargo flamegraph -p committer_cli -- commit

echo 4 | sudo tee /proc/sys/kernel/perf_event_paranoid
