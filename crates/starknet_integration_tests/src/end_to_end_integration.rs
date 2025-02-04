use std::collections::HashSet;

use starknet_api::block::BlockNumber;
use tracing::info;

use crate::sequencer_manager::IntegrationTestManagerWrapper;

pub async fn end_to_end_integration() {
    const BLOCK_TO_WAIT_FOR_FIRST_ROUND: BlockNumber = BlockNumber(10);
    const BLOCK_TO_WAIT_FOR_LATE_NODE: BlockNumber = BlockNumber(25);
    const N_TXS: usize = 50;
    /// The number of consolidated local sequencers that participate in the test.
    const N_CONSOLIDATED_SEQUENCERS: usize = 3;
    /// The number of distributed remote sequencers that participate in the test.
    const N_DISTRIBUTED_SEQUENCERS: usize = 2;

    // Get the sequencer configurations.
    let mut integration_test_manager_wrapper =
        IntegrationTestManagerWrapper::new(N_CONSOLIDATED_SEQUENCERS, N_DISTRIBUTED_SEQUENCERS)
            .await;

    // Remove the node with index 1 to simulate a late node.
    let node_indices = integration_test_manager_wrapper.node_indices.clone();
    let mut filtered_nodes = node_indices.clone();
    filtered_nodes.remove(&1);

    // Run the nodes.
    integration_test_manager_wrapper.run(filtered_nodes).await;

    // Run the first block scenario to bootstrap the accounts.
    integration_test_manager_wrapper.test_bootstrap_txs_and_verify().await;

    // Run the test.
    integration_test_manager_wrapper
        .test_invoke_txs_and_verify(N_TXS, BLOCK_TO_WAIT_FOR_FIRST_ROUND)
        .await;

    // Run the late node.
    integration_test_manager_wrapper.run(HashSet::from([1])).await;
    // Run the test.
    integration_test_manager_wrapper
        .test_invoke_txs_and_verify(N_TXS, BLOCK_TO_WAIT_FOR_LATE_NODE)
        .await;

    info!("Shutting down nodes.");
    integration_test_manager_wrapper.shutdown_nodes(node_indices);

    info!("Integration test completed successfully!");
}
