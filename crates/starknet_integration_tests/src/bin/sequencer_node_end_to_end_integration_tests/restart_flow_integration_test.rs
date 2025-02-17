use std::collections::HashSet;

use starknet_api::block::BlockNumber;
use starknet_integration_tests::integration_test_utils::integration_test_setup;
use starknet_integration_tests::sequencer_manager::IntegrationTestManager;
use tracing::info;

#[tokio::main]
async fn main() {
    integration_test_setup("restart").await;
    const BLOCK_TO_SHUTDOWN_AT: BlockNumber = BlockNumber(5);
    const BLOCK_TO_RESTART_FROM: BlockNumber = BlockNumber(10);
    const BLOCK_TO_WAIT_FOR_AFTER_RESTART: BlockNumber = BlockNumber(18);
    // TODO(Eitan): keep a steady tps throughout the test instead of sending in batches
    const N_TXS: usize = 10;
    /// The number of consolidated local sequencers that participate in the test.
    const N_CONSOLIDATED_SEQUENCERS: usize = 5;
    /// The number of distributed remote sequencers that participate in the test.
    const N_DISTRIBUTED_SEQUENCERS: usize = 0;

    // Get the sequencer configurations.
    let mut integration_test_manager = IntegrationTestManager::new(
        N_CONSOLIDATED_SEQUENCERS,
        N_DISTRIBUTED_SEQUENCERS,
        None,
        None,
    )
    .await;

    let mut node_indices = integration_test_manager.get_node_indices();
    let mut non_zero_node_indices = node_indices.iter().filter(|node_index| **node_index != 0);
    let first_node_index = *non_zero_node_indices.next().unwrap();
    let second_node_index = *non_zero_node_indices.next().unwrap();

    let first_node_hashset = {
        let mut first_node_hashset = HashSet::new();
        first_node_hashset.insert(first_node_index);
        first_node_hashset
    };
    let second_node_hashset = {
        let mut second_node_hashset = HashSet::new();
        second_node_hashset.insert(second_node_index);
        second_node_hashset
    };

    // Run the nodes.
    integration_test_manager.run_nodes(node_indices.clone()).await;
    info!("Running all nodes");

    // Run the first block scenario to bootstrap the accounts.
    integration_test_manager.send_bootstrap_txs_and_verify().await;

    // Sending transactions and verifying
    integration_test_manager.send_invoke_txs_and_verify(N_TXS, BLOCK_TO_SHUTDOWN_AT).await;

    // Shutdown the first consolidated node.
    info!("Shutting down c");
    integration_test_manager.shutdown_nodes(first_node_hashset.clone());

    // Sending transactions and verifying
    integration_test_manager.send_invoke_txs_and_verify(N_TXS, BLOCK_TO_RESTART_FROM).await;

    // Shutdown the second consolidated node to test the restart flow.
    info!("Shutting down node {second_node_index}");
    integration_test_manager.shutdown_nodes(second_node_hashset.clone());
    node_indices.remove(&second_node_hashset.into_iter().next().unwrap());

    // Restart the first consolidated node.
    info!("Restarting node {first_node_index}");
    integration_test_manager.run_nodes(first_node_hashset).await;

    // Sending transactions and verifying
    integration_test_manager
        .send_invoke_txs_and_verify(N_TXS, BLOCK_TO_WAIT_FOR_AFTER_RESTART)
        .await;

    info!("Shutting down all nodes.");
    integration_test_manager.shutdown_nodes(node_indices);

    info!("Restart flow integration test completed successfully!");
}
