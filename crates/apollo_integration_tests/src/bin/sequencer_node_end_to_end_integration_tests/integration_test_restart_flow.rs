use std::time::Duration;

use apollo_deployments::service::NodeType;
use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::integration_test_manager::{
    IntegrationTestManager,
    DEFAULT_SENDER_ACCOUNT,
};
use apollo_integration_tests::integration_test_utils::integration_test_setup;
use apollo_integration_tests::utils::NodeDescriptor;
use tracing::info;

#[tokio::main]
async fn main() {
    integration_test_setup("restart").await;
    const TIMEOUT: Duration = Duration::from_secs(30);
    const LONG_TIMEOUT: Duration = Duration::from_secs(90);
    // Node layout (index → type): 0 = consolidated, 1 = hybrid, 2 = distributed.
    // The test restarts a hybrid node and shuts down a non-consolidated (hybrid/distributed) node.
    const RESTART_NODE: usize = 1; // hybrid
    const SHUTDOWN_NODE: usize = 2; // distributed

    let node_descriptors = vec![
        NodeDescriptor::consolidated(),
        NodeDescriptor::hybrid(),
        NodeDescriptor::distributed(),
    ];

    // Get the sequencer configurations.
    let mut integration_test_manager = IntegrationTestManager::new(
        node_descriptors,
        None,
        TestIdentifier::RestartFlowIntegrationTest,
    )
    .await;

    // Assert that RESTART_NODE is a hybrid node.
    assert_eq!(integration_test_manager.get_node_type(RESTART_NODE), NodeType::Hybrid);
    // Assert that SHUTDOWN_NODE is not a consolidated node.
    assert_ne!(integration_test_manager.get_node_type(SHUTDOWN_NODE), NodeType::Consolidated);

    let mut node_indices = integration_test_manager.get_node_indices();

    info!("Running all nodes.");
    integration_test_manager.run_nodes(node_indices.clone()).await;

    integration_test_manager.send_deploy_and_invoke_txs_and_verify().await;

    integration_test_manager.send_declare_txs_and_verify().await;

    // Create a simulator for sustained transaction sending.
    let simulator = integration_test_manager.create_simulator();
    let mut tx_generator = integration_test_manager.tx_generator().snapshot();

    // Task that awaits transactions and restarts nodes in phases.
    let await_and_restart_nodes_task = async {
        info!("Awaiting transactions while all nodes are up");
        integration_test_manager.poll_all_running_nodes_received_more_txs(TIMEOUT).await;

        integration_test_manager.shutdown_nodes([RESTART_NODE].into());
        info!("Awaiting transactions while node {RESTART_NODE} is down");
        integration_test_manager.poll_all_running_nodes_received_more_txs(TIMEOUT).await;

        // We want the restarted node to rejoin the network while its building blocks to check the
        // catch-up mechanism.
        integration_test_manager.run_nodes([RESTART_NODE].into()).await;
        info!(
            "Awaiting node {RESTART_NODE} to join consensus after it was restarted and before \
             node {SHUTDOWN_NODE} is shut down"
        );

        integration_test_manager
            .poll_node_reaches_consensus_decisions_after_restart(RESTART_NODE, LONG_TIMEOUT)
            .await;

        integration_test_manager.poll_all_running_nodes_received_more_txs(TIMEOUT).await;

        // Shutdown a second node to test that the restarted node has joined consensus (the network
        // can't reach consensus without the restarted node if the second node is down).
        integration_test_manager.shutdown_nodes([SHUTDOWN_NODE].into());
        // Shutting down a node that's already down results in an error so we remove it from the set
        // here.
        node_indices.remove(&SHUTDOWN_NODE);
        info!(
            "Awaiting transactions while node {RESTART_NODE} is up and node {SHUTDOWN_NODE} is \
             down"
        );
        integration_test_manager.poll_all_running_nodes_received_more_txs(LONG_TIMEOUT).await;
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
    info!("Restart flow integration test completed successfully!");
}
