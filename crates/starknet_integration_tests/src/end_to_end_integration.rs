use starknet_api::block::BlockNumber;
use tracing::info;

use crate::sequencer_manager::IntegrationTestManager;

pub async fn end_to_end_integration() {
    const BLOCK_TO_WAIT_FOR_HAPPY_FLOW: BlockNumber = BlockNumber(15);
    const N_TXS: usize = 50;
    /// The number of consolidated local sequencers that participate in the test.
    const N_CONSOLIDATED_SEQUENCERS: usize = 3;
    /// The number of distributed remote sequencers that participate in the test.
    const N_DISTRIBUTED_SEQUENCERS: usize = 2;

    // Get the sequencer configurations.
    let mut integration_test_manager =
        IntegrationTestManager::new(N_CONSOLIDATED_SEQUENCERS, N_DISTRIBUTED_SEQUENCERS).await;

    let node_indices = integration_test_manager.node_indices.clone();
    // Run the nodes.
    integration_test_manager.run_nodes(node_indices.clone()).await;

    // Run the first block scenario to bootstrap the accounts.
    integration_test_manager.send_bootstrap_txs_and_verify().await;

    // Run the test.
    integration_test_manager.send_invoke_txs_and_verify(N_TXS, BLOCK_TO_WAIT_FOR_HAPPY_FLOW).await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices);

    info!("Integration test completed successfully!");
}
