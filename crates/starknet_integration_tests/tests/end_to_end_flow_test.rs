use std::collections::HashMap;

use futures::StreamExt;
use mempool_test_utils::in_ci;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use papyrus_network::network_manager::BroadcastTopicChannels;
use papyrus_protobuf::consensus::{
    HeightAndRound,
    ProposalFin,
    ProposalInit,
    ProposalPart,
    StreamMessage,
    StreamMessageBody,
};
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::consensus_transaction::ConsensusTransaction;
use starknet_api::core::ChainId;
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::{TransactionHash, TransactionHasher, TransactionVersion};
use starknet_consensus::types::ValidatorId;
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
    ExpectedContentId,
    TestTxHashesFn,
    ACCOUNT_ID_0,
    UNDEPLOYED_ACCOUNT_ID,
};
use starknet_sequencer_infra::trace_util::configure_tracing;
use tracing::debug;

const INITIAL_HEIGHT: BlockNumber = BlockNumber(0);
const LAST_HEIGHT: BlockNumber = BlockNumber(4);
const LAST_HEIGHT_FOR_MANY_TXS: BlockNumber = BlockNumber(1);

struct TestBlockScenario {
    height: BlockNumber,
    create_rpc_txs_fn: CreateRpcTxsFn,
    test_tx_hashes_fn: TestTxHashesFn,
    expected_content_id: ExpectedContentId,
}

#[fixture]
fn tx_generator() -> MultiAccountTransactionGenerator {
    create_flow_test_tx_generator()
}

#[rstest]
#[case::end_to_end_flow(
    TestIdentifier::EndToEndFlowTest,
    create_test_blocks(),
    GasAmount(29000000),
    LAST_HEIGHT
)]
#[case::many_txs_scenario(
    TestIdentifier::EndToEndFlowTestManyTxs,
    create_test_blocks_for_many_txs_scenario(),
    GasAmount(17000000),
    LAST_HEIGHT_FOR_MANY_TXS
)]
#[tokio::test]
async fn end_to_end_flow(
    mut tx_generator: MultiAccountTransactionGenerator,
    #[case] test_identifier: TestIdentifier,
    #[case] test_blocks_scenarios: Vec<TestBlockScenario>,
    #[case] block_max_capacity_sierra_gas: GasAmount,
    #[case] expected_last_height: BlockNumber,
) {
    // TODO(yair): Remove once sporadic error in CI is solved.
    if in_ci() {
        std::env::set_var("RUST_LOG", "starknet=debug,infra=off");
    }
    configure_tracing().await;

    const LISTEN_TO_BROADCAST_MESSAGES_TIMEOUT: std::time::Duration =
        std::time::Duration::from_secs(50);
    // Setup.
    let mut mock_running_system = FlowTestSetup::new_from_tx_generator(
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

    // Build multiple heights to ensure heights are committed.
    for TestBlockScenario { height, create_rpc_txs_fn, test_tx_hashes_fn, expected_content_id } in
        test_blocks_scenarios
    {
        debug!("Starting height {}.", height);
        // Create and send transactions.
        let expected_batched_tx_hashes = run_test_scenario(
            &mut tx_generator,
            create_rpc_txs_fn,
            &mut |tx| sequencer_to_add_txs.assert_add_tx_success(tx),
            test_tx_hashes_fn,
        )
        .await;
        let expected_validator_id = expected_proposer_iter
            .next()
            .unwrap()
            .node_config
            .consensus_manager_config
            .consensus_config
            .validator_id;
        // TODO(Dan, Itay): Consider adding a utility function that waits for something to happen.
        let chain_id = mock_running_system.chain_id().clone();
        tokio::time::timeout(
            LISTEN_TO_BROADCAST_MESSAGES_TIMEOUT,
            listen_to_broadcasted_messages(
                &mut mock_running_system.consensus_proposals_channels,
                &expected_batched_tx_hashes,
                height,
                expected_content_id,
                expected_validator_id,
                &chain_id,
            ),
        )
        .await
        .expect("listen to broadcasted messages should finish in time");
    }

    for sequencer in sequencers {
        let height = sequencer.batcher_height().await;
        assert_eq!(
            height, expected_last_height,
            "Sequencer {} didn't reach last height.",
            sequencer.node_index
        );
    }
}

fn create_test_blocks() -> Vec<TestBlockScenario> {
    let next_height = INITIAL_HEIGHT.unchecked_next();
    let heights_to_build = next_height.iter_up_to(LAST_HEIGHT.unchecked_next());
    let test_scenarios: Vec<(CreateRpcTxsFn, TestTxHashesFn, ExpectedContentId)> = vec![
        (
            create_multiple_account_txs,
            test_multiple_account_txs,
            ExpectedContentId::from_hex_unchecked(
                "0x64c5ebaca7d9cd1e1950e3bcca96a7ec835d1b3eaab470ab880e3c6f723067e",
            ),
        ),
        (
            create_funding_txs,
            test_single_tx,
            ExpectedContentId::from_hex_unchecked(
                "0x6b19dbd8035e9f6655ab71e4157ddc0f4d09eee19bb90334ba2acc6b25511d3",
            ),
        ),
        (
            deploy_account_and_invoke,
            test_two_txs,
            ExpectedContentId::from_hex_unchecked(
                "0x4304d4abbfc005198a358530ed806820ae0bbd45199bb051947f9221eb2d51b",
            ),
        ),
        (
            create_declare_tx,
            test_single_tx,
            ExpectedContentId::from_hex_unchecked(
                "0x4bc25e55d7e8515c39cd4837b49c9be5a82ea5fbbba306ac7c7b020eac7b7f6",
            ),
        ),
    ];
    itertools::zip_eq(heights_to_build, test_scenarios)
        .map(|(height, (create_rpc_txs_fn, test_tx_hashes_fn, expected_content_id))| {
            TestBlockScenario { height, create_rpc_txs_fn, test_tx_hashes_fn, expected_content_id }
        })
        .collect()
}

fn create_test_blocks_for_many_txs_scenario() -> Vec<TestBlockScenario> {
    let next_height = INITIAL_HEIGHT.unchecked_next();
    let heights_to_build = next_height.iter_up_to(LAST_HEIGHT_FOR_MANY_TXS.unchecked_next());
    let test_scenarios: Vec<(CreateRpcTxsFn, TestTxHashesFn, ExpectedContentId)> = vec![
        // Note: The following test scenario sends 15 transactions but only 12 are included in the
        // block. This means that the last 3 transactions could be included in the next block if
        // one is added to the test.
        (
            create_many_invoke_txs,
            test_many_invoke_txs,
            ExpectedContentId::from_hex_unchecked(
                "0x412540c4c46aee449f0534bc28ef9f9f8432ebeb84a0b4142d36dd5ade48d6e",
            ),
        ),
    ];
    itertools::zip_eq(heights_to_build, test_scenarios)
        .map(|(height, (create_rpc_txs_fn, test_tx_hashes_fn, expected_content_id))| {
            TestBlockScenario { height, create_rpc_txs_fn, test_tx_hashes_fn, expected_content_id }
        })
        .collect()
}

async fn wait_for_sequencer_node(sequencer: &FlowSequencerSetup) {
    sequencer.monitoring_client.await_alive(5000, 50).await.expect("Node should be alive.");
}

async fn listen_to_broadcasted_messages(
    consensus_proposals_channels: &mut BroadcastTopicChannels<
        StreamMessage<ProposalPart, HeightAndRound>,
    >,
    expected_batched_tx_hashes: &[TransactionHash],
    expected_height: BlockNumber,
    expected_content_id: ExpectedContentId,
    expected_proposer_id: ValidatorId,
    chain_id: &ChainId,
) {
    let broadcasted_messages_receiver =
        &mut consensus_proposals_channels.broadcasted_messages_receiver;
    // Collect messages in a map so that validations will use the ordering defined by `message_id`,
    // meaning we ignore network reordering, like the StreamHandler.
    let mut messages_cache = HashMap::new();
    let mut last_message_id = 0;

    while let Some((Ok(message), _)) = broadcasted_messages_receiver.next().await {
        if message.stream_id.0 == expected_height.0 {
            messages_cache.insert(message.message_id, message.clone());
        } else {
            panic!(
                "Expected height: {}. Received message from unexpected height: {}",
                expected_height.0, message.stream_id.0
            );
        }
        if message.message == papyrus_protobuf::consensus::StreamMessageBody::Fin {
            last_message_id = message.message_id;
        }
        // Check that we got the Fin message and all previous messages.
        if last_message_id > 0 && (0..=last_message_id).all(|id| messages_cache.contains_key(&id)) {
            break;
        }
    }
    // TODO(Dan, Guy): retrieve / calculate the expected proposal init and fin.
    let expected_proposal_init = ProposalInit {
        height: expected_height,
        proposer: expected_proposer_id,
        ..Default::default()
    };
    let expected_proposal_fin = ProposalFin { proposal_commitment: BlockHash(expected_content_id) };

    let StreamMessage {
        stream_id: first_stream_id,
        message: init_message,
        message_id: incoming_message_id,
    } = messages_cache.remove(&0).expect("Stream is missing its first message");

    assert_eq!(
        incoming_message_id, 0,
        "Expected the first message in the stream to have id 0, got {}",
        incoming_message_id
    );
    let StreamMessageBody::Content(ProposalPart::Init(incoming_proposal_init)) = init_message
    else {
        panic!("Expected an init message. Got: {:?}", init_message)
    };
    assert_eq!(
        incoming_proposal_init, expected_proposal_init,
        "Unexpected init message: {:?}, expected: {:?}",
        incoming_proposal_init, expected_proposal_init
    );

    let mut received_tx_hashes = Vec::new();
    let mut got_proposal_fin = false;
    let mut got_channel_fin = false;
    for i in 1_u64..messages_cache.len().try_into().unwrap() {
        let StreamMessage { message, stream_id, message_id: _ } =
            messages_cache.remove(&i).expect("Stream should have all consecutive messages");
        assert_eq!(stream_id, first_stream_id, "Expected the same stream id for all messages");
        match message {
            StreamMessageBody::Content(ProposalPart::Init(init)) => {
                panic!("Unexpected init: {:?}", init)
            }
            StreamMessageBody::Content(ProposalPart::Fin(proposal_fin)) => {
                assert_eq!(
                    proposal_fin, expected_proposal_fin,
                    "Unexpected fin message: {:?}, expected: {:?}",
                    proposal_fin, expected_proposal_fin
                );
                got_proposal_fin = true;
            }
            StreamMessageBody::Content(ProposalPart::BlockInfo(_)) => {
                // TODO(Asmaa): Add validation for block info.
            }
            StreamMessageBody::Content(ProposalPart::Transactions(transactions)) => {
                // TODO(Arni): add calculate_transaction_hash to consensus transaction and use it
                // here.
                received_tx_hashes.extend(transactions.transactions.iter().map(|tx| match tx {
                    ConsensusTransaction::RpcTransaction(tx) => {
                        let starknet_api_tx =
                            starknet_api::transaction::Transaction::from(tx.clone());
                        starknet_api_tx.calculate_transaction_hash(chain_id).unwrap()
                    }
                    ConsensusTransaction::L1Handler(tx) => {
                        tx.calculate_transaction_hash(chain_id, &TransactionVersion::ZERO).unwrap()
                    }
                }));
            }
            StreamMessageBody::Fin => {
                got_channel_fin = true;
            }
        }
        if got_proposal_fin && got_channel_fin {
            assert!(
                received_tx_hashes.len() == expected_batched_tx_hashes.len(),
                "Expected {} transactions, got {}",
                expected_batched_tx_hashes.len(),
                received_tx_hashes.len()
            );
            break;
        }
    }

    received_tx_hashes.sort();
    let mut expected_batched_tx_hashes = expected_batched_tx_hashes.to_vec();
    expected_batched_tx_hashes.sort();
    assert_eq!(received_tx_hashes, expected_batched_tx_hashes, "Unexpected transactions");
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
