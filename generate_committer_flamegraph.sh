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
echo 2 | sudo tee /proc/sys/kernel/perf_event_paranoid

gcloud storage cat gs://committer-testing-artifacts/23ffcf5/committer_flow_inputs.json | jq -r .committer_input | CARGO_PROFILE_RELEASE_DEBUG=true cargo flamegraph -p committer_cli -- commit

echo 4 | sudo tee /proc/sys/kernel/perf_event_paranoid
