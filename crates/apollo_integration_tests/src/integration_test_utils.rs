use apollo_sequencer_infra::trace_util::configure_tracing;
use apollo_sequencer_node::test_utils::node_runner::get_node_executable_path;
use tracing::{info, warn};

// TODO(Tsabary): remove the hook definition once we transition to proper usage of task
// spawning.
pub fn set_panic_hook() {
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_panic(info);
        std::process::exit(1);
    }));
}

pub async fn integration_test_setup(test_specifier: &str) {
    configure_tracing().await;
    info!("Running sequencer node end to end {test_specifier} flow integration test setup.");
    set_panic_hook();

    let sequencer_path = get_node_executable_path();
    warn!(
        "This test uses a compiled sequencer node binary located at {sequencer_path}. Make sure \
         to pre-compile the binary before running this test. Alternatively, you can compile the \
         binary and run this test with './scripts/sequencer_integration_test.sh {test_specifier}'"
    );
}
