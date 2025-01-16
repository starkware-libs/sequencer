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
    let sequencers_setup = get_sequencer_setup_configs(tx_generator).await;

    // Run the sequencers.
    // TODO(Nadin, Tsabary): Refactor to separate the construction of SequencerManager from its
    // invocation. Consider using the builder pattern.
    let sequencer_manager = SequencerSetupManager::run(sequencers_setup).await;

    // Run the integration test simulator and verify the results.
    sequencer_manager
        .test_and_verify(tx_generator, N_TXS, SENDER_ACCOUNT, sender_address, EXPECTED_BLOCK_NUMBER)
        .await;

    info!("Shutting down nodes.");
    sequencer_manager.shutdown_nodes();
}
