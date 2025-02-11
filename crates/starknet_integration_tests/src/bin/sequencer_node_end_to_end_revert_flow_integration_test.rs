use std::thread::sleep;
use std::time::Duration;

use starknet_api::block::BlockNumber;
use starknet_integration_tests::integration_test_utils::integration_test_setup;
use starknet_integration_tests::sequencer_manager::IntegrationTestManager;
use tracing::info;

#[tokio::main]
async fn main() {
    integration_test_setup("revert_flow", "End to end revert flow integration").await;
    end_to_end_revert_flow_integration().await;
}

pub async fn end_to_end_revert_flow_integration() {
    const BLOCK_TO_REVERT_TO: BlockNumber = BlockNumber(10);
    const BLOCK_TO_REVERT_FROM: BlockNumber = BlockNumber(20);
    const BLOCK_TO_WAIT_FOR_AFTER_REVERT: BlockNumber = BlockNumber(25);
    const N_TXS: usize = 50;
    /// The number of consolidated local sequencers that participate in the test.
    const N_CONSOLIDATED_SEQUENCERS: usize = 1;
    /// The number of distributed remote sequencers that participate in the test.
    const N_DISTRIBUTED_SEQUENCERS: usize = 0;

    // Get the sequencer configurations.
    let mut integration_test_manager =
        IntegrationTestManager::new(N_CONSOLIDATED_SEQUENCERS, N_DISTRIBUTED_SEQUENCERS, None)
            .await;

    let node_indices = integration_test_manager.get_node_indices();

    integration_test_manager.run_nodes(node_indices.clone()).await;
    // Run the first block scenario to bootstrap the accounts.
    integration_test_manager.send_bootstrap_txs_and_verify().await;
    integration_test_manager.send_invoke_txs_and_verify(N_TXS, BLOCK_TO_REVERT_TO).await;
    // Snapshot the tx generator so we can restore it after the revert.
    let tx_gen_snapshot = integration_test_manager.tx_generator().snapshot();
    integration_test_manager.send_invoke_txs_and_verify(N_TXS, BLOCK_TO_REVERT_FROM).await;
    integration_test_manager.shutdown_nodes(node_indices.clone());
    integration_test_manager.update_revert_config_to_all_idle_nodes(Some(BLOCK_TO_REVERT_TO));
    integration_test_manager.run_nodes(node_indices.clone()).await;
    // allow the nodes to revert the blocks.
    sleep(Duration::from_secs(5));
    integration_test_manager.shutdown_nodes(node_indices.clone());
    integration_test_manager.update_revert_config_to_all_idle_nodes(None);
    *integration_test_manager.tx_generator_mut() = tx_gen_snapshot;
    integration_test_manager.run_nodes(node_indices.clone()).await;
    integration_test_manager
        .send_invoke_txs_and_verify(N_TXS, BLOCK_TO_WAIT_FOR_AFTER_REVERT)
        .await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices);

    info!("Revert flow integration test completed successfully!");
}
