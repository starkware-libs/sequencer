use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::integration_test_manager::IntegrationTestManager;
use apollo_integration_tests::integration_test_utils::integration_test_setup;
use starknet_api::block::BlockNumber;
use tracing::info;

#[tokio::main]
async fn main() {
    integration_test_setup("single_node").await;
    const BLOCK_TO_WAIT_FOR: BlockNumber = BlockNumber(10000);
    const N_INVOKE_TXS: usize = 10000;
    const N_L1_HANDLER_TXS: usize = 2;
    /// The number of consolidated local sequencers that participate in the test.
    const N_CONSOLIDATED_SEQUENCERS: usize = 1;
    /// The number of distributed remote sequencers that participate in the test.
    const N_DISTRIBUTED_SEQUENCERS: usize = 0;

    // Get the sequencer configurations.
    let mut integration_test_manager = IntegrationTestManager::new(
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

    integration_test_manager.send_declare_txs_and_verify().await;

    // Run the test.
    integration_test_manager
        .send_txs_and_verify(N_INVOKE_TXS, N_L1_HANDLER_TXS, BLOCK_TO_WAIT_FOR)
        .await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices);

    info!("Single node integration test completed successfully!");
}
