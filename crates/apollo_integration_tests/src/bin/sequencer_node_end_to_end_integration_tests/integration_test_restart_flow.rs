use std::time::Duration;

use apollo_deployments::deployments::consolidated::ConsolidatedNodeServiceName;
use apollo_deployments::deployments::hybrid::HybridNodeServiceName;
use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::integration_test_manager::{
    IntegrationTestManager,
    DEFAULT_SENDER_ACCOUNT,
};
use apollo_integration_tests::integration_test_utils::integration_test_setup;
use apollo_integration_tests::utils::ConsensusTxs;
use strum::IntoEnumIterator;
use tokio::select;
use tracing::info;

#[tokio::main]
async fn main() {
    integration_test_setup("restart").await;
    const TIMEOUT: Duration = Duration::from_secs(30);
    const LONG_TIMEOUT: Duration = Duration::from_secs(90);
    /// The number of consolidated local sequencers that participate in the test.
    const N_CONSOLIDATED_SEQUENCERS: usize = 1;
    /// The number of distributed remote sequencers that participate in the test.
    const N_DISTRIBUTED_SEQUENCERS: usize = 1;
    /// The number of hybrid sequencers that participate in the test.
    const N_HYBRID_SEQUENCERS: usize = 1;
    // The indices of the nodes that we will be shutting down.
    // The test restarts a hybrid node and shuts down a non-consolidated (hybrid/distributed) node.
    const RESTART_NODE: usize = N_CONSOLIDATED_SEQUENCERS;
    const SHUTDOWN_NODE: usize = RESTART_NODE + 1;

    // Get the sequencer configurations.
    let mut integration_test_manager = IntegrationTestManager::new(
        N_CONSOLIDATED_SEQUENCERS,
        N_DISTRIBUTED_SEQUENCERS,
        N_HYBRID_SEQUENCERS,
        None,
        TestIdentifier::RestartFlowIntegrationTest,
    )
    .await;

    // Assert that RESTART_NODE is a hybrid node.
    assert_eq!(
        integration_test_manager
            .get_idle_nodes()
            .get(&RESTART_NODE)
            .unwrap()
            .get_executables()
            .len(),
        HybridNodeServiceName::iter().count()
    );
    // Assert that SHUTDOWN_NODE is not a consolidated node.
    assert_ne!(
        integration_test_manager
            .get_idle_nodes()
            .get(&SHUTDOWN_NODE)
            .unwrap()
            .get_executables()
            .len(),
        ConsolidatedNodeServiceName::iter().count()
    );

    let mut node_indices = integration_test_manager.get_node_indices();

    info!(
        "Running all nodes: {N_CONSOLIDATED_SEQUENCERS} consolidated and \
         {N_DISTRIBUTED_SEQUENCERS} distributed sequencers"
    );
    integration_test_manager.run_nodes(node_indices.clone()).await;

    integration_test_manager.send_deploy_and_invoke_txs_and_verify().await;

    integration_test_manager.send_declare_txs_and_verify().await;

    // Create a simulator for sustained transaction sending.
    let simulator = integration_test_manager.create_simulator();
    let mut tx_generator = integration_test_manager.tx_generator().snapshot();
    let test_scenario = ConsensusTxs { n_invoke_txs: 1, n_l1_handler_txs: 0 };
    // TODO(noamsp/itay): Try to refactor this test to spawn threads for each task instead of using
    // select!
    // Task that sends sustained transactions without ever finishing.
    let continuous_tx_sending_task =
        simulator.send_txs_continuously(&mut tx_generator, &test_scenario, DEFAULT_SENDER_ACCOUNT);

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

    select! {
        _ = continuous_tx_sending_task => {
            panic!("This task should never complete");
        }
        _ = await_and_restart_nodes_task => {
            // If await_and_restart_nodes_task completes normally, it finished successfully.
            // If it panicked, the panic would have already propagated.
        }
    }

    integration_test_manager.shutdown_nodes(node_indices);
    info!("Restart flow integration test completed successfully!");
}
