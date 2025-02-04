use std::collections::HashSet;

use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use starknet_api::block::BlockNumber;
use tracing::info;

use crate::sequencer_manager::IntegrationTestManager;

pub async fn end_to_end_integration(tx_generator: &mut MultiAccountTransactionGenerator) {
    const BLOCK_TO_WAIT_FOR_FIRST_ROUND: BlockNumber = BlockNumber(10);
    const BLOCK_TO_WAIT_FOR_LATE_NODE: BlockNumber = BlockNumber(25);
    const N_TXS: usize = 50;
    /// The number of consolidated local sequencers that participate in the test.
    const N_CONSOLIDATED_SEQUENCERS: usize = 3;
    /// The number of distributed remote sequencers that participate in the test.
    const N_DISTRIBUTED_SEQUENCERS: usize = 2;

    // Get the sequencer configurations.
    let (mut integration_test_manager, node_indices) =
        IntegrationTestManager::setup_nodes_and_create_manager(
            tx_generator,
            N_CONSOLIDATED_SEQUENCERS,
            N_DISTRIBUTED_SEQUENCERS,
        )
        .await;

    // Remove the node with index 1 to simulate a late node.
    let mut filtered_nodes = node_indices.clone();
    filtered_nodes.remove(&1);

    // Run the nodes.
    integration_test_manager.run(filtered_nodes).await;

    // Run the first block scenario to bootstrap the accounts.
    integration_test_manager.bootstrap(tx_generator).await;

    // Run the test.
    integration_test_manager.send_invoke_txs(tx_generator, N_TXS).await;
    integration_test_manager
        .verify_blocks_on_running_nodes(tx_generator, BLOCK_TO_WAIT_FOR_FIRST_ROUND)
        .await;

    // Run the late node.
    integration_test_manager.run(HashSet::from([1])).await;
    integration_test_manager.send_invoke_txs(tx_generator, N_TXS).await;
    integration_test_manager
        .verify_blocks_on_running_nodes(tx_generator, BLOCK_TO_WAIT_FOR_LATE_NODE)
        .await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices);

    info!("Integration test completed successfully!");
}
