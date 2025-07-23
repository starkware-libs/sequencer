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
use apollo_integration_tests::monitoring_utils::get_consensus_decisions_reached;
use apollo_integration_tests::utils::{ConsensusTxs, N_TXS_IN_FIRST_BLOCK};
use strum::IntoEnumIterator;
use tokio::join;
use tokio::time::{sleep, Instant};
use tracing::info;

#[tokio::main]
async fn main() {
    integration_test_setup("restart").await;
    const TIMEOUT: Duration = Duration::from_secs(30);
    const LONG_TIMEOUT: Duration = Duration::from_secs(90);
    const TOTAL_INVOKE_TXS: u64 = 250;
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
        poll_running_nodes_received_more_txs(
            &mut nodes_accepted_txs_mapping,
            &integration_test_manager,
            TIMEOUT,
        )
        .await;

        integration_test_manager.shutdown_nodes([RESTART_NODE].into());
        info!("Awaiting transactions while node {RESTART_NODE} is down");
        poll_running_nodes_received_more_txs(
            &mut nodes_accepted_txs_mapping,
            &integration_test_manager,
            TIMEOUT,
        )
        .await;

        // We want the restarted node to rejoin the network while its building blocks to check the
        // catch-up mechanism.
        integration_test_manager.run_nodes([RESTART_NODE].into()).await;
        info!(
            "Awaiting node {RESTART_NODE} joins consensus after it was restarted and before node \
             {SHUTDOWN_NODE} is shut down"
        );

        let verify_duration = poll_node_reaches_consensus_decisions_after_restarts(
            &integration_test_manager,
            RESTART_NODE,
            LONG_TIMEOUT,
        )
        .await;

        poll_running_nodes_received_more_txs(
            &mut nodes_accepted_txs_mapping,
            &integration_test_manager,
            LONG_TIMEOUT - verify_duration,
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
        poll_running_nodes_received_more_txs(
            &mut nodes_accepted_txs_mapping,
            &integration_test_manager,
            LONG_TIMEOUT,
        )
        .await;

        // The expected accepted number of transactions is the total number of invoke transactions
        // sent + the number of transactions sent during the bootstrap phase.
        let expected_n_accepted_txs = N_TXS_IN_FIRST_BLOCK
            + TryInto::<usize>::try_into(TOTAL_INVOKE_TXS)
                .expect("Failed to convert TOTAL_INVOKE_TXS to usize");
        // TODO(lev): We need to find a way to stop sending transactions after the test is done and
        // not to wait till all transactions are sent and processed.

        info!(
            "Verifying that all running nodes processed all transactions - \
             {expected_n_accepted_txs}"
        );
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
async fn poll_running_nodes_received_more_txs(
    prev_txs: &mut HashMap<usize, usize>,
    integration_test_manager: &IntegrationTestManager,
    timeout: Duration,
) {
    let mut curr_processed_txs = prev_txs.clone();

    let start = Instant::now();
    let mut done: bool = false;
    while start.elapsed() < timeout && !done {
        sleep(Duration::from_secs(1)).await;
        done =
            update_processed_txs(integration_test_manager, &mut curr_processed_txs, prev_txs).await;
    }

    info!(
        "Verifying running nodes received more transactions finished in {} seconds",
        start.elapsed().as_secs()
    );
    for (node_idx, curr_n_processed) in curr_processed_txs {
        let prev_n_processed =
            prev_txs.insert(node_idx, curr_n_processed).expect("Num txs not found");
        info!(
            "Node {} processed {} -> {} transactions",
            node_idx, prev_n_processed, curr_n_processed
        );
    }

    if !done {
        panic!(
            "Not all running nodes processed more transactions in the last {} seconds",
            timeout.as_secs()
        );
    }
}

async fn update_processed_txs(
    integration_test_manager: &IntegrationTestManager,
    curr_processed_txs: &mut HashMap<usize, usize>,
    prev_txs: &HashMap<usize, usize>,
) -> bool {
    let curr_txs = integration_test_manager.get_num_accepted_txs_on_all_running_nodes().await;
    for (node_idx, curr_n_processed) in curr_txs {
        curr_processed_txs.insert(node_idx, curr_n_processed).expect("Num txs not found");

        let prev_n_processed =
            prev_txs.get(&node_idx).expect("Node index not found in previous transactions mapping");

        if curr_n_processed <= *prev_n_processed {
            return false;
        }
    }
    true
}

// TODO(lev): Make a polling function that receives as args a condition function and a timeout.
// Then we can use it in both poll_... functions.

/// Verifies that the node with the given index reaches consensus decisions after being restarted.
async fn poll_node_reaches_consensus_decisions_after_restarts(
    integration_test_manager: &IntegrationTestManager,
    node_idx: usize,
    timeout: Duration,
) -> Duration {
    let consensus_monitoring_client = integration_test_manager
        .get_consensus_manager_monitoring_client_for_running_node(node_idx)
        .await;
    let prev_decisions_reached = get_consensus_decisions_reached(consensus_monitoring_client).await;
    let mut curr_decisions_reached = prev_decisions_reached;
    let mut done: bool = false;
    let start = Instant::now();
    while start.elapsed() < timeout && !done {
        sleep(Duration::from_secs(1)).await;
        // TODO(lev): We should use a more efficient metrics to be sure that restarted node
        // integrated back in reaching consensus.
        curr_decisions_reached = get_consensus_decisions_reached(consensus_monitoring_client).await;
        if curr_decisions_reached > prev_decisions_reached + 1 {
            done = true;
        }
    }
    info!(
        "Verifying node is reaching consensus decisions after restart finished in {} seconds",
        start.elapsed().as_secs()
    );
    info!(
        "Node {node_idx} consensus decisions reached after restart. Previous: {}, New: {}",
        prev_decisions_reached, curr_decisions_reached
    );
    if !done {
        panic!(
            "Node {node_idx} did not reach consensus decisions after restart in the last {} \
             seconds",
            timeout.as_secs()
        );
    }
    start.elapsed()
}
