use std::collections::HashMap;
use std::time::Duration;

use apollo_deployments::deployments::hybrid::HybridNodeServiceName;
use apollo_deployments::service::NodeType;
use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::integration_test_manager::{
    IntegrationTestManager,
    DEFAULT_SENDER_ACCOUNT,
};
use apollo_integration_tests::integration_test_utils::integration_test_setup;
use apollo_integration_tests::utils::NodeDescriptor;
use strum::IntoEnumIterator;
use tracing::info;

#[tokio::main]
async fn main() {
    integration_test_setup("restart_service_single_node").await;
    const TIMEOUT: Duration = Duration::from_secs(30);
    const LONG_TIMEOUT: Duration = Duration::from_secs(90);
    // Node layout (index → type): 0 = hybrid, 1 = hybrid, 2 = hybrid.
    // The test restarts the last hybrid node's service.
    const RESTART_NODE: usize = 2;

    let node_descriptors =
        vec![NodeDescriptor::hybrid(), NodeDescriptor::hybrid(), NodeDescriptor::hybrid()];

    // Get the sequencer configurations.
    let mut integration_test_manager = IntegrationTestManager::new(
        node_descriptors,
        None,
        TestIdentifier::RestartServiceSingleNodeFlowIntegrationTest,
    )
    .await;

    // Assert that RESTART_NODE is a hybrid node.
    assert_eq!(integration_test_manager.get_node_type(RESTART_NODE), NodeType::Hybrid);

    let node_indices = integration_test_manager.get_node_indices();
    integration_test_manager.run_nodes(node_indices.clone()).await;

    integration_test_manager.send_deploy_and_invoke_txs_and_verify().await;

    integration_test_manager.send_declare_txs_and_verify().await;

    // Create a simulator for sustained transaction sending.
    let simulator = integration_test_manager.create_simulator();
    let mut tx_generator = integration_test_manager.tx_generator().snapshot();

    // The indices of the nodes that are healthy throughout the test.
    let mut healthy_node_indices = node_indices.clone();
    healthy_node_indices.remove(&RESTART_NODE);
    // Task that awaits transactions and restarts nodes in phases.
    let await_and_restart_nodes_task = async {
        for hybrid_node_service in HybridNodeServiceName::iter() {
            // TODO(noamsp): Remove this once the equivocaton feature is merged.
            if hybrid_node_service == HybridNodeServiceName::Core {
                continue;
            }

            info!("Shutting down service {hybrid_node_service:?} for node {RESTART_NODE}.");
            let restart_node_service =
                HashMap::from([(RESTART_NODE, vec![hybrid_node_service.into()])]);
            integration_test_manager.shutdown_node_services(restart_node_service.clone());

            // Verify that the other nodes are still running properly.
            integration_test_manager
                .poll_running_nodes_received_more_txs(TIMEOUT, &healthy_node_indices)
                .await;

            info!("Running service {hybrid_node_service:?} for node {RESTART_NODE}.");
            integration_test_manager.run_node_services(restart_node_service.clone()).await;

            integration_test_manager
                .poll_node_reaches_consensus_decisions_after_restart(RESTART_NODE, LONG_TIMEOUT)
                .await;

            integration_test_manager.poll_all_running_nodes_received_more_txs(LONG_TIMEOUT).await;
        }
    };

    simulator
        .run_test_with_nonstop_tx_sending(
            &mut tx_generator,
            DEFAULT_SENDER_ACCOUNT,
            await_and_restart_nodes_task,
        )
        .await;

    integration_test_manager.verify_block_hash_across_all_running_nodes(None).await;

    integration_test_manager.shutdown_nodes(node_indices);
    info!("Restart service single node flow integration test completed successfully!");
}
