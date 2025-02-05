use std::collections::HashSet;
use std::time::Duration;

use mempool_test_utils::starknet_api_test_utils::{AccountId, MultiAccountTransactionGenerator};
use starknet_api::block::BlockNumber;
use starknet_sequencer_node::test_utils::node_runner::get_node_executable_path;
use tracing::info;

use crate::sequencer_manager::{get_sequencer_setup_configs, IntegrationTestManager};
use crate::utils::{BootstrapTxs, InvokeTxs};

pub async fn end_to_end_integration(tx_generator: &mut MultiAccountTransactionGenerator) {
    const BLOCK_TO_WAIT_FOR_BOOTSTRAP: BlockNumber = BlockNumber(2);
    const BLOCK_TO_WAIT_FOR_FIRST_ROUND: BlockNumber = BlockNumber(10);
    const BLOCK_TO_WAIT_FOR_LATE_NODE_TEMP: BlockNumber = BlockNumber(25);
    const BLOCK_TO_WAIT_FOR_CONSENSUS_NODE: BlockNumber = BlockNumber(40);
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
            // TODO(Yael): consider removing this parameter and take it from the tx_generator
            // instead.
            InvokeTxs(N_TXS),
            SENDER_ACCOUNT,
            BLOCK_TO_WAIT_FOR_FIRST_ROUND,
        )
        .await;

    info!("Shutting down node {:?}.", 1);
    integration_test_manager.shutdown_nodes(HashSet::from([1]));
    integration_test_manager
        .test_and_verify(
            tx_generator,
            InvokeTxs(N_TXS),
            SENDER_ACCOUNT,
            BLOCK_TO_WAIT_FOR_LATE_NODE_TEMP,
        )
        .await;

    info!("Turning node {:?} back on.", 1);
    integration_test_manager.run(HashSet::from([1])).await;

    // Shutdown the node with index 2 to ensure that the shutdown node participates in the
    // consensus.
    info!("Sleeping for 30 seconds to allow the node to catch up.");
    tokio::time::sleep(Duration::from_secs(30)).await;
    integration_test_manager.shutdown_nodes(HashSet::from([2]));

    integration_test_manager
        .test_and_verify(
            tx_generator,
            InvokeTxs(N_TXS),
            SENDER_ACCOUNT,
            BLOCK_TO_WAIT_FOR_CONSENSUS_NODE,
        )
        .await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices);

    info!("Integration test completed successfully!");
    // Kill other node so we ar sure previous one is in consensus, then run more txs
}
