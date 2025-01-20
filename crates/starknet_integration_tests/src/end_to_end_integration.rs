use mempool_test_utils::starknet_api_test_utils::{AccountId, MultiAccountTransactionGenerator};
use starknet_api::block::BlockNumber;
use starknet_sequencer_node::test_utils::node_runner::get_node_executable_path;
use tracing::info;

use crate::sequencer_manager::{get_sequencer_setup_configs, IntegrationTestManager};

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
    let integration_test_manager = IntegrationTestManager::run(sequencers_setup).await;

    // Run the integration test simulator.
    integration_test_manager
        .run_integration_test_simulator(tx_generator, N_TXS, SENDER_ACCOUNT)
        .await;

    integration_test_manager.await_execution(EXPECTED_BLOCK_NUMBER).await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes();

    // Verify the results.
    integration_test_manager.verify_results(sender_address, N_TXS).await;
}
