use mempool_test_utils::starknet_api_test_utils::{AccountId, MultiAccountTransactionGenerator};
use starknet_api::block::BlockNumber;
use starknet_sequencer_infra::test_utils::{AvailablePorts, MAX_NUMBER_OF_INSTANCES_PER_TEST};
use starknet_sequencer_node::test_utils::node_runner::get_node_executable_path;
use tracing::info;

use crate::sequencer_manager::{
    create_consolidated_sequencer_configs,
    create_distributed_node_configs,
    get_sequencer_setup_configs,
    verify_results,
    ComposedNodeComponentConfigs,
    SequencerSetupManager,
};
use crate::test_identifiers::TestIdentifier;

/// The number of consolidated local sequencers that participate in the test.
const N_CONSOLIDATED_SEQUENCERS: usize = 3;
/// The number of distributed remote sequencers that participate in the test.
const N_DISTRIBUTED_SEQUENCERS: usize = 2;

pub async fn end_to_end_integration(tx_generator: MultiAccountTransactionGenerator) {
    const EXPECTED_BLOCK_NUMBER: BlockNumber = BlockNumber(15);
    const N_TXS: usize = 50;
    const SENDER_ACCOUNT: AccountId = 0;
    let sender_address = tx_generator.account_with_id(SENDER_ACCOUNT).sender_address();

    info!("Checking that the sequencer node executable is present.");
    get_node_executable_path();

    let test_unique_id = TestIdentifier::EndToEndIntegrationTest;

    // TODO(Nadin): Assign a dedicated set of available ports to each sequencer.
    let mut available_ports =
        AvailablePorts::new(test_unique_id.into(), MAX_NUMBER_OF_INSTANCES_PER_TEST - 1);

    let component_configs: Vec<ComposedNodeComponentConfigs> = {
        let mut combined = Vec::new();
        // Create elements in place.
        combined.extend(create_consolidated_sequencer_configs(N_CONSOLIDATED_SEQUENCERS));
        combined.extend(create_distributed_node_configs(
            &mut available_ports,
            N_DISTRIBUTED_SEQUENCERS,
        ));
        combined
    };

    // Get the sequencer configurations.
    let sequencers_setup = get_sequencer_setup_configs(
        test_unique_id,
        &tx_generator,
        available_ports,
        component_configs,
    )
    .await;

    // Run the sequencers.
    // TODO(Nadin, Tsabary): Refactor to separate the construction of SequencerManager from its
    // invocation. Consider using the builder pattern.
    let sequencer_manager = SequencerSetupManager::run(sequencers_setup).await;

    // Run the integration test simulator.
    sequencer_manager.run_integration_test_simulator(tx_generator, N_TXS, SENDER_ACCOUNT).await;

    sequencer_manager.await_execution(EXPECTED_BLOCK_NUMBER).await;

    info!("Shutting down nodes.");
    sequencer_manager.shutdown_nodes();

    // TODO(AlonH): Consider checking all sequencer storage readers.
    let batcher_storage_reader = sequencer_manager.batcher_storage_reader();

    // Verify the results.
    verify_results(sender_address, batcher_storage_reader, N_TXS).await;
}
