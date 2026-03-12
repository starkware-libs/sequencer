use std::collections::HashSet;
use std::time::Duration;

use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::integration_test_manager::{IntegrationTestManager, DEFAULT_SENDER_ACCOUNT};
use apollo_integration_tests::integration_test_utils::integration_test_setup;
use apollo_integration_tests::utils::ConsensusTxs;
use apollo_node_config::definitions::ConfigPointersMap;
use apollo_node_config::node_config::SequencerNodeConfig;
use serde_json::Value;
use starknet_api::block::BlockNumber;
use tracing::info;

/// Test whether a single node performing a synchronous reorg (revert loop) can disrupt
/// the consensus of the remaining honest nodes.
///
/// Scenario:
/// 1. Run 5 validators normally until block 5
/// 2. Shut down node 4, configure it to revert to block 0, restart it
///    - Node 4 enters the synchronous revert loop (CPU churn, no tokio yield)
/// 3. Send more transactions and check if nodes 0-3 can still reach consensus
///
/// If nodes 0-3 can still produce blocks: no vulnerability (4/5 > 2/3 quorum).
/// If nodes 0-3 get stuck: the reorg loop on one node is affecting peers.
#[tokio::main]
async fn main() {
    integration_test_setup("reorg_churn").await;

    const INITIAL_BLOCK: BlockNumber = BlockNumber(5);
    const N_INVOKE_TXS: usize = 50;
    const N_L1_HANDLER_TXS: usize = 2;
    const N_CONSOLIDATED_SEQUENCERS: usize = 5;
    const N_DISTRIBUTED_SEQUENCERS: usize = 0;
    const N_HYBRID_SEQUENCERS: usize = 0;
    const CHURNING_NODE: usize = 4;

    let mut integration_test_manager = IntegrationTestManager::new(
        N_CONSOLIDATED_SEQUENCERS,
        N_DISTRIBUTED_SEQUENCERS,
        N_HYBRID_SEQUENCERS,
        None,
        TestIdentifier::ReorgChurnIntegrationTest,
    )
    .await;

    let all_nodes = integration_test_manager.get_node_indices();

    // Phase 1: Run all 5 nodes normally, deploy accounts, reach block 5.
    info!("Phase 1: Running all nodes and reaching block {}.", INITIAL_BLOCK.0);
    integration_test_manager.run_nodes(all_nodes.clone()).await;
    integration_test_manager.send_deploy_and_invoke_txs_and_verify().await;
    integration_test_manager.send_declare_txs_and_verify().await;
    integration_test_manager
        .send_txs_and_verify(N_INVOKE_TXS, N_L1_HANDLER_TXS, INITIAL_BLOCK)
        .await;
    info!("Phase 1 complete: all 5 nodes reached block {}.", INITIAL_BLOCK.0);

    // Phase 2: Make node 4 enter a real reorg (revert to block 0).
    // This triggers the synchronous revert loop that churns the CPU without yielding to tokio.
    info!("Phase 2: Shutting down node {} to configure revert.", CHURNING_NODE);
    integration_test_manager.shutdown_nodes(HashSet::from([CHURNING_NODE]));

    modify_revert_config_idle_nodes(
        &mut integration_test_manager,
        HashSet::from([CHURNING_NODE]),
        Some(BlockNumber(0)),
    );

    info!(
        "Restarting node {} with revert enabled — it will enter the synchronous revert loop.",
        CHURNING_NODE
    );
    integration_test_manager.run_nodes(HashSet::from([CHURNING_NODE])).await;

    // Phase 3: Check if the remaining 4 nodes can still make progress.
    // With 4 out of 5 validators, Byzantine consensus requires 4 > floor(5*2/3) = 3, so quorum
    // should still be reachable.
    let healthy_nodes: HashSet<usize> = (0..CHURNING_NODE).collect();
    info!(
        "Phase 3: Sending more txs and checking if nodes {:?} can still reach consensus...",
        healthy_nodes
    );

    // Send transactions through node 0.
    let simulator = integration_test_manager.create_simulator();
    simulator
        .send_txs(
            integration_test_manager.tx_generator_mut(),
            &ConsensusTxs { n_invoke_txs: 10, n_l1_handler_txs: 0 },
            DEFAULT_SENDER_ACCOUNT,
        )
        .await;

    // Verify that the healthy nodes process the new transactions.
    // Timeout of 120s — if they can't make progress, the test fails.
    integration_test_manager
        .poll_running_nodes_received_more_txs(Duration::from_secs(120), &healthy_nodes)
        .await;

    info!(
        "SUCCESS: Nodes {:?} continued processing transactions despite node {} reverting!",
        healthy_nodes, CHURNING_NODE
    );

    // Shut down only the healthy nodes. Node 4 may have already exited after completing its
    // revert (it enters eternal pending, but the process might have crashed).
    integration_test_manager.shutdown_nodes(healthy_nodes);
    info!("Reorg churn integration test completed successfully!");
}

fn modify_revert_config_idle_nodes(
    integration_test_manager: &mut IntegrationTestManager,
    node_indices: HashSet<usize>,
    revert_up_to_and_including: Option<BlockNumber>,
) {
    integration_test_manager.modify_config_pointers_idle_nodes(
        node_indices.clone(),
        |config_pointers| {
            modify_revert_config_pointers(config_pointers, revert_up_to_and_including)
        },
    );
    integration_test_manager.modify_config_idle_nodes(node_indices, |config| {
        modify_revert_config(config, revert_up_to_and_including)
    });
}

fn modify_revert_config_pointers(
    config_pointers: &mut ConfigPointersMap,
    revert_up_to_and_including: Option<BlockNumber>,
) {
    let should_revert = revert_up_to_and_including.is_some();
    config_pointers.change_target_value("revert_config.should_revert", Value::from(should_revert));
    if let Some(block) = revert_up_to_and_including {
        config_pointers
            .change_target_value("revert_config.revert_up_to_and_including", Value::from(block.0));
    }
}

fn modify_revert_config(
    config: &mut SequencerNodeConfig,
    revert_up_to_and_including: Option<BlockNumber>,
) {
    let should_revert = revert_up_to_and_including.is_some();
    config.state_sync_config.as_mut().unwrap().static_config.revert_config.should_revert =
        should_revert;
    config.consensus_manager_config.as_mut().unwrap().revert_config.should_revert = should_revert;
    if let Some(block) = revert_up_to_and_including {
        config
            .state_sync_config
            .as_mut()
            .unwrap()
            .static_config
            .revert_config
            .revert_up_to_and_including = block;
        config
            .consensus_manager_config
            .as_mut()
            .unwrap()
            .revert_config
            .revert_up_to_and_including = block;
    }
}
