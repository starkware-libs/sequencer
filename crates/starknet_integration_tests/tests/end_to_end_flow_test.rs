use std::time::Duration;

use mempool_test_utils::starknet_api_test_utils::{
    create_l1_handler_tx,
    MultiAccountTransactionGenerator,
};
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::{L1HandlerTransaction, TransactionHash};
use starknet_infra_utils::test_utils::TestIdentifier;
use starknet_integration_tests::flow_test_setup::{FlowSequencerSetup, FlowTestSetup};
use starknet_integration_tests::utils::{
    create_deploy_account_tx_and_invoke_tx,
    create_flow_test_tx_generator,
    create_funding_txs,
    create_many_invoke_txs,
    create_multiple_account_txs,
    run_test_scenario,
    test_many_invoke_txs,
    test_multiple_account_txs,
    CreateRpcTxsFn,
    TestTxHashesFn,
    ACCOUNT_ID_0,
    UNDEPLOYED_ACCOUNT_ID,
};
use starknet_sequencer_infra::trace_util::configure_tracing;
use tracing::info;

struct TestScenario {
    create_rpc_txs_fn: CreateRpcTxsFn,
    l1_handler_txs: Vec<L1HandlerTransaction>,
    test_tx_hashes_fn: TestTxHashesFn,
}

#[fixture]
fn tx_generator() -> MultiAccountTransactionGenerator {
    create_flow_test_tx_generator()
}

#[rstest]
#[case::end_to_end_flow(
    TestIdentifier::EndToEndFlowTest,
    create_test_blocks(),
    GasAmount(29000000)
)]
#[case::many_txs_scenario(
    TestIdentifier::EndToEndFlowTestManyTxs,
    create_test_blocks_for_many_txs_scenario(),
    GasAmount(17500000)
)]
#[tokio::test]
async fn end_to_end_flow(
    mut tx_generator: MultiAccountTransactionGenerator,
    #[case] test_identifier: TestIdentifier,
    #[case] test_blocks_scenarios: Vec<TestScenario>,
    #[case] block_max_capacity_sierra_gas: GasAmount,
) {
    configure_tracing().await;

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

    // Build multiple heights to ensure heights are committed.
    for (i, TestScenario { create_rpc_txs_fn, l1_handler_txs, test_tx_hashes_fn }) in
        test_blocks_scenarios.into_iter().enumerate()
    {
        info!("Starting scenario {i}.");
        // Create and send transactions.
        // TODO(Arni): move send messages to l2 into [run_test_scenario].
        mock_running_system.send_messages_to_l2(&l1_handler_txs).await;
        let mut expected_batched_tx_hashes = run_test_scenario(
            &mut tx_generator,
            create_rpc_txs_fn,
            l1_handler_txs,
            &mut send_rpc_tx_fn,
            test_tx_hashes_fn,
            &chain_id,
        )
        .await;

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
                "Expected transactions should be included in a block by now, remaining txs: \
                 {expected_batched_tx_hashes:#?}"
            )
        });
    }
}

fn create_test_blocks() -> Vec<TestScenario> {
    vec![
        // This block should be the first to be tested, as the addition of L1 handler transaction
        // does not work smoothly with the current architecture of the test.
        // TODO(Arni): Fix this. Move the L1 handler to be not the first block.
        TestScenario {
            create_rpc_txs_fn: |_| vec![],
            l1_handler_txs: vec![create_l1_handler_tx()],
            test_tx_hashes_fn: test_single_tx,
        },
        TestScenario {
            create_rpc_txs_fn: create_multiple_account_txs,
            l1_handler_txs: vec![],
            test_tx_hashes_fn: test_multiple_account_txs,
        },
        TestScenario {
            create_rpc_txs_fn: create_funding_txs,
            l1_handler_txs: vec![],
            test_tx_hashes_fn: test_single_tx,
        },
        TestScenario {
            create_rpc_txs_fn: deploy_account_and_invoke,
            l1_handler_txs: vec![],
            test_tx_hashes_fn: test_two_txs,
        },
        TestScenario {
            create_rpc_txs_fn: create_declare_tx,
            l1_handler_txs: vec![],
            test_tx_hashes_fn: test_single_tx,
        },
    ]
}

fn create_test_blocks_for_many_txs_scenario() -> Vec<TestScenario> {
    vec![TestScenario {
        create_rpc_txs_fn: create_many_invoke_txs,
        l1_handler_txs: vec![],
        test_tx_hashes_fn: test_many_invoke_txs,
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
