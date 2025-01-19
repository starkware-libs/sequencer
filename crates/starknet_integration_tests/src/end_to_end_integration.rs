use mempool_test_utils::starknet_api_test_utils::{AccountId, MultiAccountTransactionGenerator};
use starknet_api::block::BlockNumber;
use starknet_sequencer_node::test_utils::node_runner::get_node_executable_path;
use tracing::info;

use crate::sequencer_manager::{get_sequencer_setup_configs, SequencerSetupManager};

pub async fn end_to_end_integration(tx_generator: &mut MultiAccountTransactionGenerator) {
    const EXPECTED_BLOCK_NUMBER: BlockNumber = BlockNumber(15);
    const N_TXS: usize = 50;
    const SENDER_ACCOUNT: AccountId = 0;
    let sender_address = tx_generator.account_with_id(SENDER_ACCOUNT).sender_address();

    info!("Checking that the sequencer node executable is present.");
    get_node_executable_path();

    // Get the sequencer configurations.
    let (regular_sequencer_setups, delayed_sequencer_setups) =
        get_sequencer_setup_configs(tx_generator).await;

    // Run the sequencers.
    // TODO(Nadin, Tsabary): Refactor to separate the construction of SequencerManager from its
    // invocation. Consider using the builder pattern.
    info!("Running regular sequencers.");
    let regular_sequencer_manager = SequencerSetupManager::run(regular_sequencer_setups).await;

    // Run the integration test simulator and verify the results.
    regular_sequencer_manager
        .test_and_verify(tx_generator, N_TXS, SENDER_ACCOUNT, sender_address, EXPECTED_BLOCK_NUMBER)
        .await;

    // Run the delayed sequencer.
    info!("Running delayed sequencers.");
    let delayed_sequencer_manager = SequencerSetupManager::run(delayed_sequencer_setups).await;

    // Run the integration test simulator for delayed sequencer and verify the results.
    delayed_sequencer_manager
        .test_and_verify(tx_generator, N_TXS, SENDER_ACCOUNT, sender_address, EXPECTED_BLOCK_NUMBER)
        .await;

    info!("Shutting down nodes.");
    regular_sequencer_manager.shutdown_nodes();
    delayed_sequencer_manager.shutdown_nodes();
}
