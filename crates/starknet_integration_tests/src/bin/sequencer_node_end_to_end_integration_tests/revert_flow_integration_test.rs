use std::time::Duration;

use starknet_api::block::BlockNumber;
use starknet_infra_utils::test_utils::TestIdentifier;
use starknet_integration_tests::integration_test_utils::integration_test_setup;
use starknet_integration_tests::sequencer_manager::{
    IntegrationTestManager,
    BLOCK_TO_WAIT_FOR_BOOTSTRAP,
};
use tracing::info;

#[tokio::main]
async fn main() {
    integration_test_setup("revert").await;
    const BLOCK_TO_REVERT_FROM: BlockNumber = BlockNumber(10);
    const N_TXS: usize = 50;
    /// The number of consolidated local sequencers that participate in the test.
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
        None,
        TestIdentifier::EndToEndRevertFlowIntegrationTest,
    )
    .await;

    let node_indices = integration_test_manager.get_node_indices();

    integration_test_manager.run_nodes(node_indices.clone()).await;
    // Run the first block scenario to bootstrap the accounts.
    integration_test_manager.send_bootstrap_txs_and_verify().await;
    integration_test_manager.send_invoke_txs_and_verify(N_TXS, BLOCK_TO_REVERT_FROM).await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices.clone());
    integration_test_manager
        .update_revert_config_to_all_idle_nodes(Some(BLOCK_TO_WAIT_FOR_BOOTSTRAP.unchecked_next()));
    integration_test_manager.run_nodes(node_indices.clone()).await;
    // Wait for the revert to complete.
    integration_test_manager
        .await_revert_all_running_nodes(
            BLOCK_TO_WAIT_FOR_BOOTSTRAP,
            AWAIT_REVERT_TIMEOUT_DURATION,
            AWAIT_REVERT_INTERVAL_MS,
            MAX_ATTEMPTS,
        )
        .await;
    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices.clone());

    // TODO(noamsp): Rerun nodes, send and verify transactions after the revert completed.

    info!("Revert flow integration test completed successfully!");
}
