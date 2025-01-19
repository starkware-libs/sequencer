use starknet_integration_tests::end_to_end_integration::end_to_end_integration;
use starknet_integration_tests::utils::create_integration_test_tx_generator;
use starknet_sequencer_infra::trace_util::configure_tracing;
use starknet_sequencer_node::test_utils::node_runner::get_node_executable_path;
use tracing::{info, warn};

#[tokio::main]
async fn main() {
    // TODO(Tsabary): remove the hook definition once we transition to proper usage of task
    // spawning.
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_panic(info);
        std::process::exit(1);
    }));

    configure_tracing().await;
    info!("Running integration test setup.");

    let sequencer_path = get_node_executable_path();
    warn!(
        "This test uses a compiled sequencer node binary located at {}. Make sure to pre-compile \
         the binary before running this test. Alternatively, you can compile the binary and run \
         this test with './scripts/sequencer_integration_test.sh'",
        sequencer_path
    );

    // Creates a multi-account transaction generator for integration test
    let mut tx_generator = create_integration_test_tx_generator();

    // Run end to end integration test.
    end_to_end_integration(&mut tx_generator).await;
}
