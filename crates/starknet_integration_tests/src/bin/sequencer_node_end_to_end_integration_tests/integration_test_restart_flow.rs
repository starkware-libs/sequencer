use std::collections::HashMap;
use std::time::Duration;

use starknet_infra_utils::test_utils::TestIdentifier;
use starknet_integration_tests::integration_test_manager::{
    IntegrationTestManager,
    DEFAULT_SENDER_ACCOUNT,
};
use starknet_integration_tests::integration_test_utils::integration_test_setup;
use starknet_integration_tests::utils::{ConsensusTxs, N_TXS_IN_FIRST_BLOCK, TPS};
use tokio::join;
use tokio::time::sleep;
use tracing::info;

const PHASE_DURATION_SECS: u64 = 30;

#[tokio::main]
async fn main() {
    integration_test_setup("restart").await;

    const N_CONSOLIDATED_SEQUENCERS: usize = 5;
    const N_DISTRIBUTED_SEQUENCERS: usize = 0;
    const NODE_1: usize = 1;
    const NODE_2: usize = 2;
    const TOTAL_PHASES: u64 = 4;
    const TOTAL_DURATION_SECS: u64 = PHASE_DURATION_SECS * TOTAL_PHASES;
    const TOTAL_INVOKE_TXS: u64 = TPS * TOTAL_DURATION_SECS;

    // Get the sequencer configurations
    let mut integration_test_manager = IntegrationTestManager::new(
        N_CONSOLIDATED_SEQUENCERS,
        N_DISTRIBUTED_SEQUENCERS,
        None,
        TestIdentifier::RestartFlowIntegrationTest,
    )
    .await;

    let mut node_indices = integration_test_manager.get_node_indices();

    info!("Running all nodes");
    integration_test_manager.run_nodes(node_indices.clone()).await;

    integration_test_manager.send_bootstrap_txs_and_verify().await;

    let mut nodes_accepted_txs_mapping =
        integration_test_manager.get_num_accepted_txs_on_all_running_nodes().await;

    // Create a simulator for sustained transaction sending
    let simulator = integration_test_manager.create_simulator();
    let mut tx_generator = integration_test_manager.tx_generator().snapshot();
    let test_scenario = ConsensusTxs {
        n_invoke_txs: TOTAL_INVOKE_TXS.try_into().expect("Failed to convert TPS to usize"),
        n_l1_handler_txs: 0,
    };

    // Spawn transaction sending task that runs for the entire test duration
    let tx_sending_task =
        simulator.send_txs(&mut tx_generator, &test_scenario, DEFAULT_SENDER_ACCOUNT);

    let await_and_restart_nodes_task = async {
        info!("Awaiting transactions while all nodes are up");
        sleep_and_verify_running_nodes_received_more_txs(
            &mut nodes_accepted_txs_mapping,
            &integration_test_manager,
        )
        .await;

        integration_test_manager.shutdown_nodes([NODE_1].into());
        info! {"Awaiting transactions while node {NODE_1} is down"}
        sleep_and_verify_running_nodes_received_more_txs(
            &mut nodes_accepted_txs_mapping,
            &integration_test_manager,
        )
        .await;

        // We want node 1 to rejoin the network while its building blocks to check the catch-up
        // mechanism.
        integration_test_manager.run_nodes([NODE_1].into()).await;
        info!(
            "Awaiting transactions after node {NODE_1} was restarted and before node {NODE_2} is \
             shut down"
        );
        sleep_and_verify_running_nodes_received_more_txs(
            &mut nodes_accepted_txs_mapping,
            &integration_test_manager,
        )
        .await;

        // Shutdown second node to test that the first node has joined consensus (the network can't
        // reach consensus without the first node if the second node is down).
        integration_test_manager.shutdown_nodes([NODE_2].into());
        info!("Awaiting transactions while node {NODE_1} is up and node {NODE_2} is down");
        sleep_and_verify_running_nodes_received_more_txs(
            &mut nodes_accepted_txs_mapping,
            &integration_test_manager,
        )
        .await;
    };

    let _ = join!(tx_sending_task, await_and_restart_nodes_task);

    // Verify that each running node processed all transactions
    // The expected total number of transactions is the total number of invoke transactions sent +
    // the number of transactions sent during the bootstrap phase
    info!("Verifying that all running nodes processed all transactions");
    integration_test_manager
        .await_txs_accepted_on_all_running_nodes(
            N_TXS_IN_FIRST_BLOCK
                + TryInto::<usize>::try_into(TOTAL_INVOKE_TXS)
                    .expect("Failed to convert TOTAL_INVOKE_TXS to usize"),
        )
        .await;

    // Shutting down a node that's already down results in an error so we remove it from the set
    // here
    node_indices.remove(&NODE_2);
    integration_test_manager.shutdown_nodes(node_indices);
    info!("Restart flow integration test completed successfully!");
}

// Sleeps for the duration of a phase and verifies that all running nodes processed more
// transactions during the sleep. The function takes a mutable reference to a mapping of the number
// of transactions processed by each node at the end of the previous phase. The function updates the
// mapping with the number of transactions processed by each node at the end of the current phase.
async fn sleep_and_verify_running_nodes_received_more_txs(
    prev_txs: &mut HashMap<usize, usize>,
    integration_test_manager: &IntegrationTestManager,
) {
    sleep(Duration::from_secs(PHASE_DURATION_SECS)).await;
    let curr_txs = integration_test_manager.get_num_accepted_txs_on_all_running_nodes().await;
    for (node_idx, curr_n_processed) in curr_txs {
        let prev_n_processed = *prev_txs.get(&node_idx).unwrap();
        info!("Node {} processed {} transactions", node_idx, curr_n_processed);
        assert!(
            curr_n_processed > prev_n_processed,
            "Node {} did not process more transactions",
            node_idx
        );
        prev_txs.insert(node_idx, curr_n_processed);
    }
}
