// Each test module is compiled as a separate crate, and all can declare the common module.
// This means that any peace of code in this module that is not used by *all* test modules will be
// identified as unused code by clippy (for one of the crates).
#![allow(dead_code)]

use std::time::Duration;

use apollo_batcher::metrics::REVERTED_TRANSACTIONS;
use apollo_infra::trace_util::configure_tracing;
use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::flow_test_setup::{
    FlowSequencerSetup,
    FlowTestSetup,
    NUM_OF_SEQUENCERS,
};
use apollo_integration_tests::utils::{
    create_flow_test_tx_generator,
    run_test_scenario,
    CreateL1ToL2MessagesArgsFn,
    CreateRpcTxsFn,
    TestTxHashesFn,
};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use pretty_assertions::assert_eq;
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::TransactionHash;
use tracing::info;

// Note: run integration/flow tests from separate files in `tests/`, which helps cargo ensure
// isolation (prevent cross-contamination of services/resources) and that these tests won't be
// parallelized (which won't work with fixed ports).
pub async fn end_to_end_flow(
    test_identifier: TestIdentifier,
    test_blocks_scenarios: Vec<TestScenario>,
    block_max_capacity_gas: GasAmount, // Used to max both sierra and proving gas.
    expecting_full_blocks: bool,
    allow_bootstrap_txs: bool,
) {
    configure_tracing().await;

    let mut tx_generator = create_flow_test_tx_generator();
    let global_recorder_handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("Should be able to install global prometheus recorder");

    const TEST_SCENARIO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(50);
    // Setup.
    let mock_running_system = FlowTestSetup::new_from_tx_generator(
        &tx_generator,
        test_identifier.into(),
        block_max_capacity_gas,
        allow_bootstrap_txs,
    )
    .await;

    tokio::join!(
        wait_for_sequencer_node(&mock_running_system.sequencer_0),
        wait_for_sequencer_node(&mock_running_system.sequencer_1),
    );

    let sequencers = [&mock_running_system.sequencer_0, &mock_running_system.sequencer_1];
    // We use only the first sequencer's gateway to test that the mempools are syncing.
    let sequencer_to_add_txs = *sequencers.first().unwrap();
    let mut expected_proposer_iter = sequencers.iter().cycle();
    // We start at height 1, so we need to skip the proposer of the initial height.
    expected_proposer_iter.next().unwrap();
    let chain_id = mock_running_system.chain_id().clone();
    let mut send_rpc_tx_fn = |tx| sequencer_to_add_txs.assert_add_tx_success(tx);

    // In this test each sequencer increases the BATCHED_TRANSACTIONS metric which tracks the number
    // of accepted transactions. This tracks the cumulative count across all sequencers and
    // scenarios.
    let mut total_expected_batched_txs_count = 0;

    // Build multiple heights to ensure heights are committed.
    for (
        i,
        TestScenario { create_rpc_txs_fn, create_l1_to_l2_messages_args_fn, test_tx_hashes_fn },
    ) in test_blocks_scenarios.into_iter().enumerate()
    {
        info!("Starting scenario {i}.");
        // Create and send transactions.
        // TODO(Arni): move send messages to l2 into [run_test_scenario].
        let l1_handlers = create_l1_to_l2_messages_args_fn(&mut tx_generator);
        mock_running_system.send_messages_to_l2(&l1_handlers).await;

        // Run the test scenario and get the expected batched tx hashes of the current scenario.
        let expected_batched_tx_hashes = run_test_scenario(
            &mut tx_generator,
            create_rpc_txs_fn,
            l1_handlers,
            &mut send_rpc_tx_fn,
            test_tx_hashes_fn,
            &chain_id,
        )
        .await;

        // Each sequencer increases the same BATCHED_TRANSACTIONS metric because they are running
        // in the same process in this test.
        total_expected_batched_txs_count += NUM_OF_SEQUENCERS * expected_batched_tx_hashes.len();
        let mut current_batched_txs_count = 0;

        tokio::time::timeout(TEST_SCENARIO_TIMEOUT, async {
            loop {
                info!(
                    "Waiting for more txs to be batched in a block. Expected batched txs: \
                     {total_expected_batched_txs_count}, Currently batched txs: \
                     {current_batched_txs_count}"
                );

                current_batched_txs_count = get_total_batched_txs_count(&global_recorder_handle);
                if current_batched_txs_count == total_expected_batched_txs_count {
                    break;
                }

                tokio::time::sleep(Duration::from_millis(2000)).await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "Scenario {i}: Expected transactions should be included in a block by now, \
                 Expected amount of batched txs: {total_expected_batched_txs_count}, Currently \
                 amount of batched txs: {current_batched_txs_count}"
            )
        });
    }

    assert_full_blocks_flow(&global_recorder_handle, expecting_full_blocks);
    assert_no_reverted_transactions_flow(&global_recorder_handle);
}

pub struct TestScenario {
    pub create_rpc_txs_fn: CreateRpcTxsFn,
    pub create_l1_to_l2_messages_args_fn: CreateL1ToL2MessagesArgsFn,
    pub test_tx_hashes_fn: TestTxHashesFn,
}

fn get_total_batched_txs_count(recorder_handle: &PrometheusHandle) -> usize {
    let metrics = recorder_handle.render();
    apollo_batcher::metrics::BATCHED_TRANSACTIONS.parse_numeric_metric::<usize>(&metrics).unwrap()
}

fn assert_full_blocks_flow(recorder_handle: &PrometheusHandle, expecting_full_blocks: bool) {
    let metrics = recorder_handle.render();
    let full_blocks_metric = apollo_batcher::metrics::BLOCK_CLOSE_REASON
        .parse_numeric_metric::<u64>(
            &metrics,
            &[(
                apollo_batcher::metrics::LABEL_NAME_BLOCK_CLOSE_REASON,
                apollo_batcher::metrics::BlockCloseReason::FullBlock.into(),
            )],
        )
        .unwrap();
    if expecting_full_blocks {
        assert!(full_blocks_metric > 0);
    } else {
        assert_eq!(full_blocks_metric, 0);
    }
}

fn assert_no_reverted_transactions_flow(recorder_handle: &PrometheusHandle) {
    let metrics = recorder_handle.render();
    let reverted_transactions_metric =
        REVERTED_TRANSACTIONS.parse_numeric_metric::<u64>(&metrics).unwrap();
    assert_eq!(reverted_transactions_metric, 0);
}

async fn wait_for_sequencer_node(sequencer: &FlowSequencerSetup) {
    sequencer.monitoring_client.await_alive(5000, 50).await.expect("Node should be alive.");
}

pub fn test_single_tx(tx_hashes: &[TransactionHash]) -> Vec<TransactionHash> {
    assert_eq!(tx_hashes.len(), 1, "Expected a single transaction");
    tx_hashes.to_vec()
}

/// TODO(Itamar): Use this function in all tests built with TestScenario struct.
#[track_caller]
pub fn validate_tx_count(
    tx_hashes: &[TransactionHash],
    expected_count: usize,
) -> Vec<TransactionHash> {
    let tx_hashes_len = tx_hashes.len();
    assert_eq!(
        tx_hashes_len, expected_count,
        "Expected {expected_count} txs, but found {tx_hashes_len} txs.",
    );
    tx_hashes.to_vec()
}
