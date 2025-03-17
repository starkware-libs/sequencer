use starknet_api::block::BlockNumber;
use starknet_infra_utils::test_utils::TestIdentifier;
use starknet_integration_tests::integration_test_manager::IntegrationTestManager;
use starknet_integration_tests::integration_test_utils::integration_test_setup;
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
    // The indices of the nodes that we will be shutting down. Node 0 is skipped because we use it
    // to verify the metrics.
    const NODE_1: usize = 1;
    const NODE_2: usize = 2;

    // Get the sequencer configurations.
    let mut integration_test_manager = IntegrationTestManager::new(
        N_CONSOLIDATED_SEQUENCERS,
        N_DISTRIBUTED_SEQUENCERS,
        None,
        TestIdentifier::RestartFlowIntegrationTest,
    )
    .await;

    let mut node_indices = integration_test_manager.get_node_indices();

    integration_test_manager.run_nodes(node_indices.clone()).await;
    info!("Running all nodes");

    integration_test_manager.send_bootstrap_txs_and_verify().await;

    integration_test_manager.send_txs_and_verify(N_TXS, 1, BLOCK_TO_SHUTDOWN_AT).await;

    info!("Network reached block {BLOCK_TO_SHUTDOWN_AT}. Shutting down node {NODE_1}");
    integration_test_manager.shutdown_nodes([NODE_1].into());

    info! {"Sending transactions while node {NODE_1} is down"}
    integration_test_manager.send_txs_and_verify(N_TXS, 1, BLOCK_TO_RESTART_FROM).await;

    // Shutdown second node to test that the first node has joined (the network can't reach
    // consensus if 2 nodes are down)
    info!("Network reached block {BLOCK_TO_RESTART_FROM}. Shutting down node {NODE_2}");
    integration_test_manager.shutdown_nodes([NODE_2].into());
    // Shutting down a node that's already down results in an error so we remove it from the set
    // here
    node_indices.remove(&NODE_2);

    info!("Restarting node {NODE_1}");
    integration_test_manager.run_nodes([NODE_1].into()).await;

    info!("Sending transactions while node {NODE_2} is down and after {NODE_1} was restarted");
    integration_test_manager.send_txs_and_verify(N_TXS, 1, BLOCK_TO_WAIT_FOR_AFTER_RESTART).await;

    info!("Shutting down all nodes.");
    integration_test_manager.shutdown_nodes(node_indices);

    info!("Restart flow integration test completed successfully!");
}
