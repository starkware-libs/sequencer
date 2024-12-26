use starknet_integration_tests::end_to_end_integration::end_to_end_integration;
use starknet_integration_tests::utils::create_integration_test_tx_generator;
use starknet_sequencer_infra::trace_util::configure_tracing;
use tracing::info;

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

    // Creates a multi-account transaction generator for integration test
    let tx_generator = create_integration_test_tx_generator();

    // Run end to end integration test.
    end_to_end_integration(tx_generator).await;
}
