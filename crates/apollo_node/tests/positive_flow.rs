use apollo_infra_utils::test_utils::TestIdentifier;
use starknet_api::block::BlockNumber;
use tracing::info;

mod common;

#[tokio::test]
async fn positive_flow() {
    common::integration_test_setup("positive").await;
    const BLOCK_TO_WAIT_FOR: BlockNumber = BlockNumber(15);
    const N_INVOKE_TXS: usize = 50;
    const N_L1_HANDLER_TXS: usize = 2;
    /// The number of consolidated local sequencers that participate in the test.
    const N_CONSOLIDATED_SEQUENCERS: usize = 3;
    /// The number of distributed remote sequencers that participate in the test.
    const N_DISTRIBUTED_SEQUENCERS: usize = 2;

    // Get the sequencer configurations.
    let mut integration_test_manager = common::IntegrationTestManager::new(
        N_CONSOLIDATED_SEQUENCERS,
        N_DISTRIBUTED_SEQUENCERS,
        None,
        TestIdentifier::PositiveFlowIntegrationTest,
    )
    .await;

    let node_indices = integration_test_manager.get_node_indices();
    // Run the nodes.
    integration_test_manager.run_nodes(node_indices.clone()).await;

    // Run the first block scenario to deploy the accounts.
    integration_test_manager.send_deploy_and_invoke_txs_and_verify().await;

    // Run the test.
    integration_test_manager
        .send_txs_and_verify(N_INVOKE_TXS, N_L1_HANDLER_TXS, BLOCK_TO_WAIT_FOR)
        .await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices);

    info!("Positive flow integration test completed successfully!");
}
