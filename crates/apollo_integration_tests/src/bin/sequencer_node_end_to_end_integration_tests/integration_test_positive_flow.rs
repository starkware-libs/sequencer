use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::integration_test_manager::IntegrationTestManager;
use apollo_integration_tests::integration_test_utils::integration_test_setup;
use starknet_api::block::BlockNumber;
use tracing::info;

#[tokio::main]
async fn main() {
    integration_test_setup("positive").await;
    const BLOCK_TO_WAIT_FOR: BlockNumber = BlockNumber(15);
    const BLOCKS_TO_WAIT_FOR_DECLARES: u64 = 5;
    const N_TXS: usize = 50;
    // TODO(Yael/Arni): 0 is a temporary value till fixing the nonce issue.
    const N_L1_HANDLER_TXS: usize = 0;
    /// The number of consolidated local sequencers that participate in the test.
    const N_CONSOLIDATED_SEQUENCERS: usize = 3;
    /// The number of distributed remote sequencers that participate in the test.
    const N_DISTRIBUTED_SEQUENCERS: usize = 2;

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

    // Run the test.
    integration_test_manager.send_txs_and_verify(N_TXS, N_L1_HANDLER_TXS, BLOCK_TO_WAIT_FOR).await;
    let bn_to_expect_declare_included =
        BlockNumber(BLOCK_TO_WAIT_FOR.0 + BLOCKS_TO_WAIT_FOR_DECLARES);
    integration_test_manager.send_declare_txs_and_verify(bn_to_expect_declare_included).await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices);

    info!("Positive flow integration test completed successfully!");
}
