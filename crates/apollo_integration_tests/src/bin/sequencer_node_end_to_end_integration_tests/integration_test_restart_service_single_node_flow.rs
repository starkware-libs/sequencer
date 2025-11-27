use std::collections::HashMap;
use std::time::Duration;

use apollo_deployments::deployments::hybrid::HybridNodeServiceName;
use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::integration_test_manager::{
    IntegrationTestManager,
    DEFAULT_SENDER_ACCOUNT,
};
use apollo_integration_tests::integration_test_utils::integration_test_setup;
use apollo_integration_tests::restart_test_utils::{
    poll_all_running_nodes_received_more_txs,
    poll_node_reaches_consensus_decisions_after_restart,
    poll_running_nodes_received_more_txs,
};
use apollo_integration_tests::utils::ConsensusTxs;
use strum::IntoEnumIterator;
use tokio::select;
use tracing::info;

#[tokio::main]
async fn main() {
    integration_test_setup("restart_service_single_node").await;
    const TIMEOUT: Duration = Duration::from_secs(30);
    const LONG_TIMEOUT: Duration = Duration::from_secs(90);
    const TOTAL_INVOKE_TXS: u64 = 250;
    /// The number of consolidated local sequencers that participate in the test.
    const N_CONSOLIDATED_SEQUENCERS: usize = 0;
    /// The number of distributed remote sequencers that participate in the test.
    const N_DISTRIBUTED_SEQUENCERS: usize = 0;
    /// The number of hybrid sequencers that participate in the test.
    const N_HYBRID_SEQUENCERS: usize = 3;
    // The indices of the nodes that we will be shutting down.
    // The test restarts a hybrid node's service and shuts down a non-consolidated
    // (hybrid/distributed) node.
    const RESTART_NODE: usize = N_HYBRID_SEQUENCERS - 1;

    // Get the sequencer configurations.
    let mut integration_test_manager = IntegrationTestManager::new(
        N_CONSOLIDATED_SEQUENCERS,
        N_DISTRIBUTED_SEQUENCERS,
        N_HYBRID_SEQUENCERS,
        None,
        TestIdentifier::RestartServiceSingleNodeFlowIntegrationTest,
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

    let node_indices = integration_test_manager.get_node_indices();
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

    // Task that sends sustained transactions for the entire test duration.
    let tx_sending_task =
        simulator.send_txs(&mut tx_generator, &test_scenario, DEFAULT_SENDER_ACCOUNT);

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
            poll_running_nodes_received_more_txs(
                &mut nodes_accepted_txs_mapping,
                &integration_test_manager,
                TIMEOUT,
                healthy_node_indices.clone(),
            )
            .await;

            info!("Running service {hybrid_node_service:?} for node {RESTART_NODE}.");
            integration_test_manager.run_node_services(restart_node_service.clone()).await;

            poll_node_reaches_consensus_decisions_after_restart(
                &integration_test_manager,
                RESTART_NODE,
                LONG_TIMEOUT,
            )
            .await;

            poll_all_running_nodes_received_more_txs(
                &mut nodes_accepted_txs_mapping,
                &integration_test_manager,
                TIMEOUT,
            )
            .await;
        }
    };

    select! {
        _ = tx_sending_task => {
            panic!("Tx sending task should not complete before the await and restart nodes task");
        }
        _ = await_and_restart_nodes_task => {
            // If await_and_restart_nodes_task completes normally, it finished successfully.
            // If it panicked, the panic would have already propagated.
        }
    }

    integration_test_manager.shutdown_nodes(node_indices);
    info!("Restart service single node flow integration test completed successfully!");
}
