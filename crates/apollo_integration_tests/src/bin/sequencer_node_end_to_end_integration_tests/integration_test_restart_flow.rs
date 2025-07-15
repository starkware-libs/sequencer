use std::collections::HashMap;
use std::time::Duration;

use apollo_deployments::deployments::consolidated::ConsolidatedNodeServiceName;
use apollo_deployments::deployments::hybrid::HybridNodeServiceName;
use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::integration_test_manager::{
    IntegrationTestManager,
    DEFAULT_SENDER_ACCOUNT,
};
use apollo_integration_tests::integration_test_utils::integration_test_setup;
use apollo_integration_tests::utils::{ConsensusTxs, N_TXS_IN_FIRST_BLOCK, TPS};
use strum::IntoEnumIterator;
use tokio::join;
use tokio::time::sleep;
use tracing::info;

#[tokio::main]
async fn main() {
    integration_test_setup("restart").await;
    const PHASE_DURATION: Duration = Duration::from_secs(10);
    const TOTAL_PHASES: u64 = 4;
    const TOTAL_DURATION: u64 = PHASE_DURATION.as_secs() * TOTAL_PHASES;
    const TOTAL_INVOKE_TXS: u64 = TPS * TOTAL_DURATION;
    /// The number of consolidated local sequencers that participate in the test.
    const N_CONSOLIDATED_SEQUENCERS: usize = 1;
    /// The number of distributed remote sequencers that participate in the test.
    const N_DISTRIBUTED_SEQUENCERS: usize = 2;
    // The indices of the nodes that we will be shutting down.
    // The test restarts a hybrid node and shuts down a non-consolidated (hybrid/distributed) node.
    const RESTART_NODE: usize = N_CONSOLIDATED_SEQUENCERS;
    const SHUTDOWN_NODE: usize = RESTART_NODE + 1;

    // Get the sequencer configurations.
    let mut integration_test_manager = IntegrationTestManager::new(
        N_CONSOLIDATED_SEQUENCERS,
        N_DISTRIBUTED_SEQUENCERS,
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

    let mut nodes_accepted_txs_mapping =
        integration_test_manager.get_num_accepted_txs_on_all_running_nodes().await;

    // Create a simulator for sustained transaction sending.
    let simulator = integration_test_manager.create_simulator();
    let mut tx_generator = integration_test_manager.tx_generator().snapshot();
    let test_scenario = ConsensusTxs {
        n_invoke_txs: TOTAL_INVOKE_TXS.try_into().expect("Failed to convert TPS to usize"),
        n_l1_handler_txs: 0,
    };

    // TODO(noamsp): Try to refactor this to use tokio::spawn.
    // Task that sends sustained transactions for the entire test duration.
    let tx_sending_task =
        simulator.send_txs(&mut tx_generator, &test_scenario, DEFAULT_SENDER_ACCOUNT);

    // Task that awaits transactions and restarts nodes in phases.
    let await_and_restart_nodes_task = async {
        info!("Awaiting transactions while all nodes are up");
        sleep(PHASE_DURATION).await;
        verify_running_nodes_received_more_txs(
            &mut nodes_accepted_txs_mapping,
            &integration_test_manager,
        )
        .await;

        integration_test_manager.shutdown_nodes([RESTART_NODE].into());
        info!("Awaiting transactions while node {RESTART_NODE} is down");
        sleep(PHASE_DURATION).await;
        verify_running_nodes_received_more_txs(
            &mut nodes_accepted_txs_mapping,
            &integration_test_manager,
        )
        .await;

        // We want the restarted node to rejoin the network while its building blocks to check the
        // catch-up mechanism.
        integration_test_manager.run_nodes([RESTART_NODE].into()).await;
        info!(
            "Awaiting transactions after node {RESTART_NODE} was restarted and before node \
             {SHUTDOWN_NODE} is shut down"
        );
        sleep(PHASE_DURATION).await;
        verify_running_nodes_received_more_txs(
            &mut nodes_accepted_txs_mapping,
            &integration_test_manager,
        )
        .await;

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
        sleep(PHASE_DURATION).await;
        verify_running_nodes_received_more_txs(
            &mut nodes_accepted_txs_mapping,
            &integration_test_manager,
        )
        .await;

        // The expected accepted number of transactions is the total number of invoke transactions
        // sent + the number of transactions sent during the bootstrap phase.
        let expected_n_accepted_txs = N_TXS_IN_FIRST_BLOCK
            + TryInto::<usize>::try_into(TOTAL_INVOKE_TXS)
                .expect("Failed to convert TOTAL_INVOKE_TXS to usize");

        info!("Verifying that all running nodes processed all transactions");
        integration_test_manager
            .await_txs_accepted_on_all_running_nodes(expected_n_accepted_txs)
            .await;
    };

    let _ = join!(tx_sending_task, await_and_restart_nodes_task);

    integration_test_manager.shutdown_nodes(node_indices);
    info!("Restart flow integration test completed successfully!");
}

// Verifies that all running nodes processed more transactions since the last check.
// Takes a mutable reference to a mapping of the number of transactions processed by each node
// at the previous check, and updates it with the current number of transactions.
async fn verify_running_nodes_received_more_txs(
    prev_txs: &mut HashMap<usize, usize>,
    integration_test_manager: &IntegrationTestManager,
) {
    let curr_txs = integration_test_manager.get_num_accepted_txs_on_all_running_nodes().await;
    for (node_idx, curr_n_processed) in curr_txs {
        let prev_n_processed =
            prev_txs.insert(node_idx, curr_n_processed).expect("Num txs not found");
        info!("Node {} processed {} transactions", node_idx, curr_n_processed);
        assert!(
            curr_n_processed > prev_n_processed,
            "Node {} did not process more transactions",
            node_idx
        );
    }
}
