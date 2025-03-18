use std::collections::HashSet;
use std::time::Duration;

use serde_json::Value;
use starknet_api::block::BlockNumber;
use starknet_infra_utils::test_utils::TestIdentifier;
use starknet_integration_tests::integration_test_manager::{
    IntegrationTestManager,
    BLOCK_TO_WAIT_FOR_BOOTSTRAP,
};
use starknet_integration_tests::integration_test_utils::integration_test_setup;
use starknet_sequencer_node::config::definitions::ConfigPointersMap;
use starknet_sequencer_node::config::node_config::SequencerNodeConfig;
use tracing::info;

#[tokio::main]
async fn main() {
    integration_test_setup("revert").await;
    const BLOCK_TO_REVERT_FROM: BlockNumber = BlockNumber(10);
    const BLOCK_TO_WAIT_FOR_AFTER_REVERT: BlockNumber = BlockNumber(15);
    // can't use static assertion as comparison is non const.
    assert!(BLOCK_TO_WAIT_FOR_BOOTSTRAP < BLOCK_TO_REVERT_FROM);
    assert!(BLOCK_TO_REVERT_FROM < BLOCK_TO_WAIT_FOR_AFTER_REVERT);

    const N_TXS: usize = 50;
    /// The number of consolidated local sequencers that participate in the test.
    // TODO(noamsp): increase N_CONSOLIDATED_SEQUENCERS to 5 once restart flow test passes.
    const N_CONSOLIDATED_SEQUENCERS: usize = 1;
    /// The number of distributed remote sequencers that participate in the test.
    const N_DISTRIBUTED_SEQUENCERS: usize = 0;

    const AWAIT_REVERT_INTERVAL_MS: u64 = 500;
    const MAX_ATTEMPTS: usize = 50;
    const AWAIT_REVERT_TIMEOUT_DURATION: Duration = Duration::from_secs(15);

    // Get the sequencer configurations.
    let mut integration_test_manager = IntegrationTestManager::new(
        N_CONSOLIDATED_SEQUENCERS,
        N_DISTRIBUTED_SEQUENCERS,
        None,
        TestIdentifier::RevertFlowIntegrationTest,
    )
    .await;

    let node_indices = integration_test_manager.get_node_indices();

    integration_test_manager.run_nodes(node_indices.clone()).await;

    info!("Sending bootstrap transactions and verifying state.");
    integration_test_manager.send_bootstrap_txs_and_verify().await;

    // Save a snapshot of the tx_generator so we can restore the state after reverting.
    let tx_generator_snapshot = integration_test_manager.tx_generator().snapshot();

    info!("Sending transactions and verifying state.");
    integration_test_manager.send_txs_and_verify(N_TXS, 1, BLOCK_TO_REVERT_FROM).await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices.clone());

    info!(
        "Changing revert config for all nodes to revert from block {BLOCK_TO_REVERT_FROM} back to \
         block {BLOCK_TO_WAIT_FOR_BOOTSTRAP}."
    );
    let revert_up_to_and_including = Some(BLOCK_TO_WAIT_FOR_BOOTSTRAP.unchecked_next());
    modify_revert_config_idle_nodes(
        &mut integration_test_manager,
        node_indices.clone(),
        revert_up_to_and_including,
    );

    integration_test_manager.run_nodes(node_indices.clone()).await;

    info!("Awaiting for all running nodes to revert back to block {BLOCK_TO_WAIT_FOR_BOOTSTRAP}.");
    integration_test_manager
        .await_revert_all_running_nodes(
            BLOCK_TO_WAIT_FOR_BOOTSTRAP,
            AWAIT_REVERT_TIMEOUT_DURATION,
            AWAIT_REVERT_INTERVAL_MS,
            MAX_ATTEMPTS,
        )
        .await;

    info!("All nodes reverted to block {BLOCK_TO_WAIT_FOR_BOOTSTRAP}. Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices.clone());

    // Restore the tx generator state.
    *integration_test_manager.tx_generator_mut() = tx_generator_snapshot;

    info!(
        "Modifying revert config for all nodes and resume sequencing from block \
         {BLOCK_TO_WAIT_FOR_BOOTSTRAP}."
    );
    modify_revert_config_idle_nodes(&mut integration_test_manager, node_indices.clone(), None);
    let node_start_height = BLOCK_TO_WAIT_FOR_BOOTSTRAP.unchecked_next();
    modify_height_configs_idle_nodes(
        &mut integration_test_manager,
        node_indices.clone(),
        node_start_height,
    );

    integration_test_manager.run_nodes(node_indices.clone()).await;

    info!("Sending transactions and verifying state.");
    integration_test_manager.send_txs_and_verify(N_TXS, 1, BLOCK_TO_WAIT_FOR_AFTER_REVERT).await;

    integration_test_manager.shutdown_nodes(node_indices);

    info!("Revert flow integration test completed successfully!");
}

// Modifies the revert config state in the given config. If `revert_up_to_and_including` is
// `None`, the revert config is disabled. Otherwise, the revert config is enabled and set
// to revert up to and including the given block number.
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
    integration_test_manager.modify_config_idle_nodes(node_indices, |config_pointers| {
        modify_revert_config(config_pointers, revert_up_to_and_including)
    });
}

fn modify_revert_config_pointers(
    config_pointers: &mut ConfigPointersMap,
    revert_up_to_and_including: Option<BlockNumber>,
) {
    let should_revert = revert_up_to_and_including.is_some();
    config_pointers.change_target_value("revert_config.should_revert", Value::from(should_revert));

    // If should revert is false, the revert_up_to_and_including value is irrelevant.
    if should_revert {
        let revert_up_to_and_including = revert_up_to_and_including.unwrap();
        config_pointers.change_target_value(
            "revert_config.revert_up_to_and_including",
            Value::from(revert_up_to_and_including.0),
        );
    }
}

fn modify_revert_config(
    config: &mut SequencerNodeConfig,
    revert_up_to_and_including: Option<BlockNumber>,
) {
    let should_revert = revert_up_to_and_including.is_some();
    config.state_sync_config.revert_config.should_revert = should_revert;
    config.consensus_manager_config.revert_config.should_revert = should_revert;

    // If should revert is false, the revert_up_to_and_including value is irrelevant.
    if should_revert {
        let revert_up_to_and_including = revert_up_to_and_including.unwrap();
        config.state_sync_config.revert_config.revert_up_to_and_including =
            revert_up_to_and_including;
        config.consensus_manager_config.revert_config.revert_up_to_and_including =
            revert_up_to_and_including;
    }
}

fn modify_height_configs_idle_nodes(
    integration_test_manager: &mut IntegrationTestManager,
    node_indices: HashSet<usize>,
    node_start_height: BlockNumber,
) {
    integration_test_manager.modify_config_idle_nodes(node_indices, |config| {
        // TODO(noamsp): Change these values point to a single config value and refactor this
        // function accordingly.
        config.consensus_manager_config.immediate_active_height = node_start_height;
        config.consensus_manager_config.cende_config.skip_write_height = Some(node_start_height);
        config.l1_provider_config.bootstrap_catch_up_height = node_start_height.prev().unwrap();
        config.l1_provider_config.provider_startup_height = node_start_height;
    });
}
