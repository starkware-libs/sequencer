use apollo_infra::trace_util::configure_tracing;
use apollo_node::test_utils::node_runner::get_node_executable_path;
use tracing::{info, warn};

// This is a change just to trigger a build and test. DO NOT MERGE THIS.

pub async fn integration_test_setup(test_specifier: &str) {
    configure_tracing().await;
    info!("Running sequencer node end to end {test_specifier} flow integration test setup.");

    let sequencer_path = get_node_executable_path();
    warn!(
        "This test uses a compiled sequencer node binary located at {sequencer_path}. Make sure \
         to pre-compile the binary before running this test. Alternatively, you can compile the \
         binary and run this test with './scripts/sequencer_integration_test.sh {test_specifier}'"
    );
}
