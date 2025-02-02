use std::collections::HashSet;

use mempool_test_utils::starknet_api_test_utils::{AccountId, MultiAccountTransactionGenerator};
use starknet_api::block::BlockNumber;
use starknet_sequencer_node::test_utils::node_runner::get_node_executable_path;
use tracing::info;

use crate::sequencer_manager::{get_sequencer_setup_configs, IntegrationTestManager};
use crate::utils::InvokeTxs;

pub async fn end_to_end_integration(tx_generator: &mut MultiAccountTransactionGenerator) {
    const EXPECTED_BLOCK_NUMBER: BlockNumber = BlockNumber(10);
    const LATE_NODE_EXPECTED_BLOCK_NUMBER: BlockNumber = BlockNumber(25);
    const N_TXS: usize = 50;
    const SENDER_ACCOUNT: AccountId = 0;
    /// The number of consolidated local sequencers that participate in the test.
    const N_CONSOLIDATED_SEQUENCERS: usize = 3;
    /// The number of distributed remote sequencers that participate in the test.
    const N_DISTRIBUTED_SEQUENCERS: usize = 2;

    info!("Checking that the sequencer node executable is present.");
    get_node_executable_path();

    // Get the sequencer configurations.
    let (sequencers_setup, node_indices) = get_sequencer_setup_configs(
        tx_generator,
        N_CONSOLIDATED_SEQUENCERS,
        N_DISTRIBUTED_SEQUENCERS,
    )
    .await;

    // Run the sequencers.
    // TODO(Nadin, Tsabary): Refactor to separate the construction of SequencerManager from its
    // invocation. Consider using the builder pattern.
    let mut integration_test_manager = IntegrationTestManager::new(sequencers_setup, Vec::new());

    // Remove the node with index 1 to simulate a late node.
    let mut filtered_nodes = node_indices.clone();
    filtered_nodes.remove(&1);

    // Run the nodes.
    integration_test_manager.run(filtered_nodes).await;

    // Run the test.
    integration_test_manager
        .test_and_verify(tx_generator, 0, InvokeTxs(N_TXS), SENDER_ACCOUNT, EXPECTED_BLOCK_NUMBER)
        .await;

    // Run the late node.
    integration_test_manager.run(HashSet::from([1])).await;

    // Run the tests after the late node joins.
    integration_test_manager
        .test_and_verify(
            tx_generator,
            N_TXS,
            InvokeTxs(N_TXS),
            SENDER_ACCOUNT,
            LATE_NODE_EXPECTED_BLOCK_NUMBER,
        )
        .await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices);

    info!("Integration test completed successfully!");
}
