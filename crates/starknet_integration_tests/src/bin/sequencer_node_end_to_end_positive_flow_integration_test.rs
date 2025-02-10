use starknet_api::block::BlockNumber;
use starknet_integration_tests::integration_test_utils::{
    set_ephemeral_port_range,
    set_panic_hook,
};
use starknet_integration_tests::sequencer_manager::IntegrationTestManager;
use starknet_sequencer_infra::trace_util::configure_tracing;
use starknet_sequencer_node::test_utils::node_runner::get_node_executable_path;
use tracing::{info, warn};

#[tokio::main]
async fn main() {
    configure_tracing().await;
    info!("Running positive flow integration test setup.");
    set_ephemeral_port_range();
    set_panic_hook();

    let sequencer_path = get_node_executable_path();
    warn!(
        "This test uses a compiled sequencer node binary located at {}. Make sure to pre-compile \
         the binary before running this test. Alternatively, you can compile the binary and run \
         this test with './scripts/sequencer_integration_test.sh'",
        sequencer_path
    );

    // Run end to end integration test.
    end_to_end_positive_flow_integration().await;
}

pub async fn end_to_end_positive_flow_integration() {
    const BLOCK_TO_WAIT_FOR: BlockNumber = BlockNumber(15);
    const N_TXS: usize = 50;
    /// The number of consolidated local sequencers that participate in the test.
    const N_CONSOLIDATED_SEQUENCERS: usize = 3;
    /// The number of distributed remote sequencers that participate in the test.
    const N_DISTRIBUTED_SEQUENCERS: usize = 2;

    // Get the sequencer configurations.
    let mut integration_test_manager = IntegrationTestManager::new(
        N_CONSOLIDATED_SEQUENCERS,
        N_DISTRIBUTED_SEQUENCERS,
        None,
        None,
    )
    .await;

    let node_indices = integration_test_manager.get_node_indices();
    // Run the nodes.
    integration_test_manager.run_nodes(node_indices.clone()).await;

    // Run the first block scenario to bootstrap the accounts.
    integration_test_manager.send_bootstrap_txs_and_verify().await;

    // Run the test.
    integration_test_manager.send_invoke_txs_and_verify(N_TXS, BLOCK_TO_WAIT_FOR).await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices);

    info!("Positive flow integration test completed successfully!");
}
