use std::collections::HashSet;

use starknet_api::block::BlockNumber;
use starknet_integration_tests::integration_test_utils::integration_test_setup;
use starknet_integration_tests::sequencer_manager::{IntegrationTestManager, NodeType};
use tracing::info;

#[tokio::main]
async fn main() {
    integration_test_setup("restart").await;
    const BLOCK_TO_RESTART_AT: BlockNumber = BlockNumber(5);
    const BLOCK_TO_REVERT_FROM: BlockNumber = BlockNumber(15);
    const BLOCK_TO_WAIT_FOR_AFTER_REVERT: BlockNumber = BlockNumber(25);
    const N_TXS: usize = 10;
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

    let node_indices_to_node_types = integration_test_manager.get_node_indices_to_node_types();
    let mut node_indices: HashSet<usize> = node_indices_to_node_types.clone().into_keys().collect();
    let mut consolidated_nodes = node_indices_to_node_types
        .into_iter()
        .filter(|(node_index, node_type)| *node_type == NodeType::Consolidated && *node_index != 0)
        .map(|(node_index, _)| node_index);
    let first_consolidated_node = {
        let mut first_consolidated_node = HashSet::new();
        first_consolidated_node.insert(consolidated_nodes.next().unwrap());
        first_consolidated_node
    };
    let second_consolidated_node = {
        let mut second_consolidated_node = HashSet::new();
        second_consolidated_node.insert(consolidated_nodes.next().unwrap());
        second_consolidated_node
    };

    // Run the nodes.
    integration_test_manager.run_nodes(node_indices.clone()).await;

    // Run the first block scenario to bootstrap the accounts.
    integration_test_manager.send_bootstrap_txs_and_verify().await;

    // Sending transactions and verifying
    integration_test_manager.send_invoke_txs_and_verify(N_TXS, BLOCK_TO_RESTART_AT).await;

    // Shutdown the first consolidated node.
    integration_test_manager.shutdown_nodes(first_consolidated_node.clone());

    // Sending transactions and verifying
    integration_test_manager.send_invoke_txs_and_verify(N_TXS, BLOCK_TO_REVERT_FROM).await;

    // Restart the first consolidated node.
    integration_test_manager.run_nodes(first_consolidated_node).await;

    // Allow first node to catch up
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;

    // Shutdown the second consolidated node to test the restart flow.
    integration_test_manager.shutdown_nodes(second_consolidated_node.clone());
    node_indices.remove(&second_consolidated_node.into_iter().next().unwrap());

    // Sending transactions and verifying
    integration_test_manager
        .send_invoke_txs_and_verify(N_TXS, BLOCK_TO_WAIT_FOR_AFTER_REVERT)
        .await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices);

    info!("Restart flow integration test completed successfully!");
}
