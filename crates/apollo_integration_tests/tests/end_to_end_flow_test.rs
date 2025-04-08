use std::time::Duration;

use apollo_infra::trace_util::configure_tracing;
use apollo_infra_utils::test_utils::TestIdentifier;
use apollo_integration_tests::flow_test_setup::{FlowSequencerSetup, FlowTestSetup};
use apollo_integration_tests::utils::{
    create_deploy_account_tx_and_invoke_tx,
    create_flow_test_tx_generator,
    create_funding_txs,
    create_l1_to_l2_message_args,
    create_many_invoke_txs,
    create_multiple_account_txs,
    run_test_scenario,
    test_many_invoke_txs,
    test_multiple_account_txs,
    CreateL1ToL2MessagesArgsFn,
    CreateRpcTxsFn,
    TestTxHashesFn,
    ACCOUNT_ID_0,
    UNDEPLOYED_ACCOUNT_ID,
};
use mempool_test_utils::starknet_api_test_utils::{
    generate_bootstrap_declare,
    MultiAccountTransactionGenerator,
};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusRecorder};
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use tracing::info;

struct TestScenario {
    create_rpc_txs_fn: CreateRpcTxsFn,
    create_l1_to_l2_messages_args_fn: CreateL1ToL2MessagesArgsFn,
    test_tx_hashes_fn: TestTxHashesFn,
}

#[fixture]
fn tx_generator() -> MultiAccountTransactionGenerator {
    create_flow_test_tx_generator()
}

#[rstest]
#[case::end_to_end_flow(
    TestIdentifier::EndToEndFlowTest,
    create_test_scenarios(),
    GasAmount(29000000),
    false
)]
#[case::many_txs_scenario(
    TestIdentifier::EndToEndFlowTestManyTxs,
    create_many_txs_scenario(),
    GasAmount(17500000),
    true
)]
#[case::bootstrap_declare_scenario(
    TestIdentifier::EndToEndFlowTestBootstrapDeclare,
    create_bootstrap_declare_scenario(),
    GasAmount(29000000),
    false
)]
#[tokio::test]
async fn end_to_end_flow(
    mut tx_generator: MultiAccountTransactionGenerator,
    #[case] test_identifier: TestIdentifier,
    #[case] test_blocks_scenarios: Vec<TestScenario>,
    #[case] block_max_capacity_sierra_gas: GasAmount,
    #[case] expecting_full_blocks: bool,
) {
    configure_tracing().await;
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);

    const TEST_SCENARIO_TIMOUT: std::time::Duration = std::time::Duration::from_secs(50);
    // Setup.
    let mock_running_system = FlowTestSetup::new_from_tx_generator(
        &tx_generator,
        test_identifier.into(),
        block_max_capacity_sierra_gas,
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
    let mut total_expected_txs = vec![];

    // Build multiple heights to ensure heights are committed.
    for (
        i,
        TestScenario { create_rpc_txs_fn, create_l1_to_l2_messages_args_fn, test_tx_hashes_fn },
    ) in test_blocks_scenarios.into_iter().enumerate()
    {
        info!("Starting scenario {i}.");
        // Create and send transactions.
        // TODO(Arni): move send messages to l2 into [run_test_scenario].
        let l1_to_l2_messages_args = create_l1_to_l2_messages_args_fn(&mut tx_generator);
        mock_running_system.send_messages_to_l2(&l1_to_l2_messages_args).await;
        let mut expected_batched_tx_hashes = run_test_scenario(
            &mut tx_generator,
            create_rpc_txs_fn,
            l1_to_l2_messages_args,
            &mut send_rpc_tx_fn,
            test_tx_hashes_fn,
            &chain_id,
        )
        .await;
        total_expected_txs.append(&mut expected_batched_tx_hashes.clone());

        tokio::time::timeout(TEST_SCENARIO_TIMOUT, async {
            loop {
                info!(
                    "Waiting for sent txs to be included in a block: {:#?}",
                    expected_batched_tx_hashes
                );

                let batched_txs =
                    &mock_running_system.accumulated_txs.lock().await.accumulated_tx_hashes;
                expected_batched_tx_hashes.retain(|tx| !batched_txs.contains(tx));
                if expected_batched_tx_hashes.is_empty() {
                    break;
                }

                tokio::time::sleep(Duration::from_millis(2000)).await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "Scenario {i}: Expected transactions should be included in a block by now, \
                 remaining txs: {expected_batched_tx_hashes:#?}"
            )
        });
    }

    assert_only_expected_txs(
        total_expected_txs,
        mock_running_system.accumulated_txs.lock().await.accumulated_tx_hashes.clone(),
    );
    assert_full_blocks_flow(&recorder, expecting_full_blocks);
}

fn assert_only_expected_txs(
    mut total_expected_txs: Vec<TransactionHash>,
    mut batched_txs: Vec<TransactionHash>,
) {
    total_expected_txs.sort();
    batched_txs.sort();
    assert_eq!(total_expected_txs, batched_txs);
}

fn assert_full_blocks_flow(recorder: &PrometheusRecorder, expecting_full_blocks: bool) {
    let metrics = recorder.handle().render();
    let full_blocks_metric =
        apollo_batcher::metrics::FULL_BLOCKS.parse_numeric_metric::<u64>(&metrics).unwrap();
    if expecting_full_blocks {
        assert!(full_blocks_metric > 0);
    } else {
        assert_eq!(full_blocks_metric, 0);
    }
}

fn create_test_scenarios() -> Vec<TestScenario> {
    vec![
        // This block should be the first to be tested, as the addition of L1 handler transaction
        // does not work smoothly with the current architecture of the test.
        // TODO(Arni): Fix this. Move the L1 handler to be not the first block.
        TestScenario {
            create_rpc_txs_fn: |_| vec![],
            create_l1_to_l2_messages_args_fn: create_l1_to_l2_message_args,
            test_tx_hashes_fn: test_single_tx,
        },
        TestScenario {
            create_rpc_txs_fn: create_multiple_account_txs,
            create_l1_to_l2_messages_args_fn: |_| vec![],
            test_tx_hashes_fn: test_multiple_account_txs,
        },
        TestScenario {
            create_rpc_txs_fn: create_funding_txs,
            create_l1_to_l2_messages_args_fn: |_| vec![],
            test_tx_hashes_fn: test_single_tx,
        },
        TestScenario {
            create_rpc_txs_fn: deploy_account_and_invoke,
            create_l1_to_l2_messages_args_fn: |_| vec![],
            test_tx_hashes_fn: test_two_txs,
        },
        TestScenario {
            create_rpc_txs_fn: create_declare_tx,
            create_l1_to_l2_messages_args_fn: |_| vec![],
            test_tx_hashes_fn: test_single_tx,
        },
    ]
}

fn create_many_txs_scenario() -> Vec<TestScenario> {
    vec![TestScenario {
        create_rpc_txs_fn: create_many_invoke_txs,
        create_l1_to_l2_messages_args_fn: |_| vec![],
        test_tx_hashes_fn: test_many_invoke_txs,
    }]
}

fn create_bootstrap_declare_scenario() -> Vec<TestScenario> {
    vec![TestScenario {
        create_rpc_txs_fn: |_| vec![generate_bootstrap_declare()],
        create_l1_to_l2_messages_args_fn: |_| vec![],
        test_tx_hashes_fn: test_single_tx,
    }]
}

async fn wait_for_sequencer_node(sequencer: &FlowSequencerSetup) {
    sequencer.monitoring_client.await_alive(5000, 50).await.expect("Node should be alive.");
}

/// Generates a deploy account transaction followed by an invoke transaction from the same deployed
/// account.
fn deploy_account_and_invoke(
    tx_generator: &mut MultiAccountTransactionGenerator,
) -> Vec<RpcTransaction> {
    create_deploy_account_tx_and_invoke_tx(tx_generator, UNDEPLOYED_ACCOUNT_ID)
}

fn test_single_tx(tx_hashes: &[TransactionHash]) -> Vec<TransactionHash> {
    assert_eq!(tx_hashes.len(), 1, "Expected a single transaction");
    tx_hashes.to_vec()
}

fn test_two_txs(tx_hashes: &[TransactionHash]) -> Vec<TransactionHash> {
    assert_eq!(tx_hashes.len(), 2, "Expected two transactions");
    tx_hashes.to_vec()
}

fn create_declare_tx(tx_generator: &mut MultiAccountTransactionGenerator) -> Vec<RpcTransaction> {
    let account_tx_generator = tx_generator.account_with_id_mut(ACCOUNT_ID_0);
    let declare_tx = account_tx_generator.generate_declare();
    vec![declare_tx]
}
