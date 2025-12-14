use std::collections::HashMap;

use apollo_deployments::deployments::hybrid::HybridNodeServiceName;
use apollo_deployments::service::NodeService;
use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::integration_test_manager::IntegrationTestManager;
use apollo_integration_tests::integration_test_utils::integration_test_setup;
use starknet_api::block::BlockNumber;
use static_assertions::const_assert;
use strum::IntoEnumIterator;
use tracing::info;

#[tokio::main]
async fn main() {
    integration_test_setup("restart_service_multiple_nodes").await;
    const INITIAL_BLOCK_TO_WAIT_FOR: usize = 20;
    const BLOCK_TO_WAIT_FOR_INCREMENT: usize = 5;
    const N_INVOKE_TXS: usize = 20;
    const N_L1_HANDLER_TXS: usize = 1;
    /// The number of consolidated local sequencers that participate in the test.
    const N_CONSOLIDATED_SEQUENCERS: usize = 0;
    /// The number of distributed remote sequencers that participate in the test.
    const N_DISTRIBUTED_SEQUENCERS: usize = 0;
    /// The number of hybrid sequencers that participate in the test.
    const N_HYBRID_SEQUENCERS: usize = 5;

    // This test assumes that there are no consolidated or distributed sequencers.
    const_assert!(N_CONSOLIDATED_SEQUENCERS == 0);
    const_assert!(N_DISTRIBUTED_SEQUENCERS == 0);

    // Get the sequencer configurations.
    let mut integration_test_manager = IntegrationTestManager::new(
        N_CONSOLIDATED_SEQUENCERS,
        N_DISTRIBUTED_SEQUENCERS,
        N_HYBRID_SEQUENCERS,
        None,
        TestIdentifier::RestartServiceMultipleNodesFlowIntegrationTest,
    )
    .await;

    let node_indices = integration_test_manager.get_node_indices();
    // Run the nodes.
    integration_test_manager.run_nodes(node_indices.clone()).await;

    // Run the first block scenario to deploy the accounts.
    integration_test_manager.send_deploy_and_invoke_txs_and_verify().await;

    integration_test_manager.send_declare_txs_and_verify().await;
    for (i, hybrid_node_service) in HybridNodeServiceName::iter().enumerate() {
        // TODO(noamsp): Remove this once the equivocaton feature is merged.
        if hybrid_node_service == HybridNodeServiceName::Core {
            continue;
        }

        let node_services_to_shutdown = node_indices
            .iter()
            .map(|&node_index| (node_index, vec![hybrid_node_service.into()]))
            .collect::<HashMap<usize, Vec<NodeService>>>();

        info!("Shutting down service {hybrid_node_service:?} for all nodes.");
        integration_test_manager.shutdown_node_services(node_services_to_shutdown.clone());
        info!("Running service {hybrid_node_service:?} for all nodes.");
        integration_test_manager.run_node_services(node_services_to_shutdown).await;

        let block_to_wait_for = BlockNumber(
            (INITIAL_BLOCK_TO_WAIT_FOR + i * BLOCK_TO_WAIT_FOR_INCREMENT)
                .try_into()
                .expect("Failed to convert to u64"),
        );
        info!(
            "Sending txs and verifying after restarting service {hybrid_node_service:?} for all \
             nodes."
        );
        integration_test_manager
            .send_txs_and_verify(N_INVOKE_TXS, N_L1_HANDLER_TXS, block_to_wait_for)
            .await;
    }

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices);

    info!("Restart service multiple nodes flow integration test completed successfully!");
}
