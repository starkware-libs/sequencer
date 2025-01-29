use std::collections::HashSet;

use mempool_test_utils::starknet_api_test_utils::{AccountId, MultiAccountTransactionGenerator};
use starknet_api::block::BlockNumber;
use starknet_sequencer_node::test_utils::node_runner::get_node_executable_path;
use tracing::info;

use crate::sequencer_manager::{get_sequencer_setup_configs, IntegrationTestManager};
use crate::utils::{FirstBlock, InvokeTxs, N_TXS_IN_FIRST_BLOCK};

pub async fn end_to_end_integration(tx_generator: &mut MultiAccountTransactionGenerator) {
    const BLOCK_TO_WAIT_FOR_BOOTSTRAP: BlockNumber = BlockNumber(2);
    const BLOCK_TO_WAIT_FOR_FIRST_ROUND: BlockNumber = BlockNumber(10);
    const BLOCK_TO_WAIT_FOR_LATE_NODE: BlockNumber = BlockNumber(25);
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

    // Run the first block scenario to bootstrap the accounts.
    integration_test_manager
        .test_and_verify(tx_generator, 0, FirstBlock, SENDER_ACCOUNT, BLOCK_TO_WAIT_FOR_BOOTSTRAP
)
        .await;

    // Run the test.
    integration_test_manager
        .test_and_verify(
            tx_generator,
            // TODO(Yael): consider removing this parameter and take it from the tx_generator
            // instead.
            N_TXS_IN_FIRST_BLOCK,
            InvokeTxs(N_TXS),
            SENDER_ACCOUNT,
            BLOCK_TO_WAIT_FOR_FIRST_ROUND,
        )
        .await;

    // Run the late node.
    integration_test_manager.run(HashSet::from([1])).await;

    // Run the tests after the late node joins.
    integration_test_manager
        .test_and_verify(
            tx_generator,
            N_TXS + N_TXS_IN_FIRST_BLOCK,
            InvokeTxs(N_TXS),
            SENDER_ACCOUNT,
            BLOCK_TO_WAIT_FOR_LATE_NODE,
        )
        .await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices);

    info!("Integration test completed successfully!");
}
