use std::collections::HashMap;

use futures::StreamExt;
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
use papyrus_storage::test_utils::CHAIN_ID_FOR_TESTS;
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::consensus_transaction::ConsensusTransaction;
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
    UNDEPLOYED_ACCOUNT_ID,
};
use starknet_sequencer_infra::trace_util::configure_tracing;
use tracing::debug;

const INITIAL_HEIGHT: BlockNumber = BlockNumber(0);
const LAST_HEIGHT: BlockNumber = BlockNumber(4);

#[fixture]
fn tx_generator() -> MultiAccountTransactionGenerator {
    create_flow_test_tx_generator()
}

#[rstest]
#[tokio::test]
async fn end_to_end_flow(mut tx_generator: MultiAccountTransactionGenerator) {
    configure_tracing().await;

    const LISTEN_TO_BROADCAST_MESSAGES_TIMEOUT: std::time::Duration =
        std::time::Duration::from_secs(50);
    // Setup.
    let mut mock_running_system = FlowTestSetup::new_from_tx_generator(
        &tx_generator,
        TestIdentifier::EndToEndFlowTest.into(),
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
    for (height, create_rpc_txs_fn, test_tx_hashes_fn, expected_content_id) in create_test_blocks()
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
        tokio::time::timeout(
            LISTEN_TO_BROADCAST_MESSAGES_TIMEOUT,
            listen_to_broadcasted_messages(
                &mut mock_running_system.consensus_proposals_channels,
                &expected_batched_tx_hashes,
                height,
                expected_content_id,
                expected_validator_id,
            ),
        )
        .await
        .expect("listen to broadcasted messages should finish in time");
    }
}

fn create_test_blocks() -> Vec<(BlockNumber, CreateRpcTxsFn, TestTxHashesFn, ExpectedContentId)> {
    let next_height = INITIAL_HEIGHT.unchecked_next();
    let heights_to_build = next_height.iter_up_to(LAST_HEIGHT.unchecked_next());
    let test_scenarios: Vec<(CreateRpcTxsFn, TestTxHashesFn, ExpectedContentId)> = vec![
        (
            create_multiple_account_txs,
            test_multiple_account_txs,
            ExpectedContentId::from_hex_unchecked(
                "0x665101f416fd5c4e91083fa9dcac1dba9a282f5211a1a2ad7695e95cb35d6b",
            ),
        ),
        (
            create_funding_txs,
            test_single_tx,
            ExpectedContentId::from_hex_unchecked(
                "0x354a08374de0b194773930010006a0cc42f7f984f509ceb0d564da37ed15bab",
            ),
        ),
        (
            deploy_account_and_invoke,
            test_two_txs,
            ExpectedContentId::from_hex_unchecked(
                "0xb28fc13e038eaff29d46d8ead91e9a37e004949c3ea6b78020c5df315ef745",
            ),
        ),
        // Note: The following test scenario sends 15 transactions but only 12 are included in the
        // block. This means that the last 3 transactions could be included in the next block if
        // one is added to the test.
        (
            create_many_invoke_txs,
            test_many_invoke_txs,
            ExpectedContentId::from_hex_unchecked(
                "0x4c490b06c1479e04c535342d4036f797444c23484f3eb53a419e361c88fcdae",
            ),
        ),
    ];
    itertools::zip_eq(heights_to_build, test_scenarios)
        .map(|(h, (create_txs_fn, test_tx_hashes_fn, expected_content_id))| {
            (h, create_txs_fn, test_tx_hashes_fn, expected_content_id)
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
) {
    let chain_id = CHAIN_ID_FOR_TESTS.clone();
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
                        starknet_api_tx.calculate_transaction_hash(&chain_id).unwrap()
                    }
                    ConsensusTransaction::L1Handler(tx) => {
                        tx.calculate_transaction_hash(&chain_id, &TransactionVersion::ZERO).unwrap()
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
