use std::collections::HashSet;

use mempool_test_utils::starknet_api_test_utils::{AccountId, MultiAccountTransactionGenerator};
use starknet_api::block::BlockNumber;
use starknet_sequencer_node::test_utils::node_runner::get_node_executable_path;
use tracing::info;

use crate::sequencer_manager::{get_sequencer_setup_configs, IntegrationTestManager};

pub async fn end_to_end_integration(tx_generator: &mut MultiAccountTransactionGenerator) {
    const EXPECTED_BLOCK_NUMBER: BlockNumber = BlockNumber(15);
    const LATE_NODE_EXPECTED_BLOCK_NUMBER: BlockNumber = BlockNumber(25);
    const N_TXS: usize = 50;
    const SENDER_ACCOUNT: AccountId = 0;

    info!("Checking that the sequencer node executable is present.");
    get_node_executable_path();

    // Get the sequencer configurations.
    let (sequencers_setup, mut node_indices) = get_sequencer_setup_configs(tx_generator).await;

    // Run the sequencers.
    // TODO(Nadin, Tsabary): Refactor to separate the construction of SequencerManager from its
    // invocation. Consider using the builder pattern.
    let mut integration_test_manager = IntegrationTestManager::new(sequencers_setup, Vec::new());

    // Remove the node with index 1 to simulate a late node.
    node_indices.remove(&1);

    // Run the nodes.
    integration_test_manager.run(node_indices).await;

    // Run the test.
    integration_test_manager
        .test_and_verify(tx_generator, 0, N_TXS, SENDER_ACCOUNT, EXPECTED_BLOCK_NUMBER)
        .await;

    // Run the late node.
    integration_test_manager.run(HashSet::from([1])).await;

    // Run the tests after the late node joins.
    integration_test_manager
        .test_and_verify(
            tx_generator,
            N_TXS,
            N_TXS,
            SENDER_ACCOUNT,
            LATE_NODE_EXPECTED_BLOCK_NUMBER,
        )
        .await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes();
}
