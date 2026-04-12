use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::integration_test_manager::IntegrationTestManager;
use apollo_integration_tests::integration_test_utils::integration_test_setup;
use apollo_integration_tests::utils::NodeDescriptor;
use starknet_api::block::BlockNumber;
use tracing::info;

#[tokio::main]
async fn main() {
    integration_test_setup("positive").await;
    const BLOCK_TO_WAIT_FOR: BlockNumber = BlockNumber(15);
    const N_INVOKE_TXS: usize = 50;
    const N_L1_HANDLER_TXS: usize = 2;

    let node_descriptors = vec![
        NodeDescriptor::consolidated(),
        NodeDescriptor::consolidated(),
        NodeDescriptor::consolidated(),
        NodeDescriptor::distributed(),
        NodeDescriptor::hybrid(),
    ];

    // Get the sequencer configurations.
    let mut integration_test_manager = IntegrationTestManager::new(
        node_descriptors,
        None,
        TestIdentifier::PositiveFlowIntegrationTest,
    )
    .await;

    // TODO(Tsabary): consider decreasing
    // "consensus_manager_config.consensus_manager_config.static_config.startup_delay" and
    // "batcher_config.static_config.block_builder_config.proposer_idle_detection_delay_millis".

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

    integration_test_manager
        .verify_block_hash_across_all_running_nodes(Some(BLOCK_TO_WAIT_FOR.unchecked_next()))
        .await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices);

    info!("Positive flow integration test completed successfully!");
}
