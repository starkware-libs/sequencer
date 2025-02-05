use mempool_test_utils::starknet_api_test_utils::{AccountId, MultiAccountTransactionGenerator};
use starknet_api::block::BlockNumber;
use starknet_sequencer_node::test_utils::node_runner::get_node_executable_path;
use tracing::info;

use crate::sequencer_manager::{get_sequencer_setup_configs, IntegrationTestManager};
use crate::utils::{BootstrapTxs, InvokeTxs};

pub async fn end_to_end_integration(tx_generator: &mut MultiAccountTransactionGenerator) {
    const BLOCK_TO_WAIT_FOR_BOOTSTRAP: BlockNumber = BlockNumber(2);
    const BLOCK_TO_WAIT_FOR_HAPPY_FLOW: BlockNumber = BlockNumber(15);
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
        None,
    )
    .await;

    // Run the sequencers.
    // TODO(Nadin, Tsabary): Refactor to separate the construction of SequencerManager from its
    // invocation. Consider using the builder pattern.
    let mut integration_test_manager = IntegrationTestManager::new(sequencers_setup, Vec::new());

    // Run the nodes.
    integration_test_manager.run(node_indices.clone()).await;

    // Run the first block scenario to bootstrap the accounts.
    integration_test_manager
        .test_and_verify(tx_generator, BootstrapTxs, SENDER_ACCOUNT, BLOCK_TO_WAIT_FOR_BOOTSTRAP)
        .await;

    // Run the test.
    integration_test_manager
        .test_and_verify(
            tx_generator,
            InvokeTxs(N_TXS),
            SENDER_ACCOUNT,
            BLOCK_TO_WAIT_FOR_HAPPY_FLOW,
        )
        .await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices);

    info!("Integration test completed successfully!");
}
