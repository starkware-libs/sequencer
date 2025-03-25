use serde_json::Value;
use starknet_api::block::BlockNumber;
use starknet_infra_utils::test_utils::TestIdentifier;
use starknet_integration_tests::integration_test_manager::IntegrationTestManager;
use starknet_integration_tests::integration_test_utils::integration_test_setup;
use tracing::info;

#[tokio::main]
async fn main() {
    integration_test_setup("positive").await;
    const BLOCK_TO_WAIT_FOR: BlockNumber = BlockNumber(15);
    const N_TXS: usize = 50;
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

    integration_test_manager.modify_config_pointers_idle_nodes(
        node_indices.clone(),
        |config_pointers| {
            config_pointers
                .change_target_value("recorder_url", Value::from("http://localhost:9714"))
        },
    );
    integration_test_manager.modify_config_idle_nodes(node_indices.clone(), |config| {
        config.consensus_manager_config.cende_config.recorder_url =
            url::Url::parse("http://localhost:9714").unwrap();
        config.consensus_manager_config.cende_config.skip_write_height = None;
    });

    // Run the nodes.
    integration_test_manager.run_nodes(node_indices.clone()).await;

    // Run the first block scenario to bootstrap the accounts.
    integration_test_manager.send_bootstrap_txs_and_verify().await;

    // Run the test.
    integration_test_manager.send_txs_and_verify(N_TXS, 2, BLOCK_TO_WAIT_FOR).await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices);

    info!("Positive flow integration test completed successfully!");
}
