use futures::StreamExt;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use papyrus_consensus::types::ValidatorId;
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
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_infra_utils::test_utils::TestIdentifier;
use starknet_integration_tests::flow_test_setup::{FlowSequencerSetup, FlowTestSetup};
use starknet_integration_tests::utils::{
    create_funding_txs,
    create_integration_test_tx_generator,
    create_many_invoke_txs,
    create_multiple_account_txs,
    run_test_scenario,
    test_many_invoke_txs,
    test_multiple_account_txs,
    UNDEPLOYED_ACCOUNT_ID,
};
use starknet_sequencer_infra::trace_util::configure_tracing;
use starknet_types_core::felt::Felt;
use tracing::debug;

const INITIAL_HEIGHT: BlockNumber = BlockNumber(0);
const LAST_HEIGHT: BlockNumber = BlockNumber(4);

#[fixture]
fn tx_generator() -> MultiAccountTransactionGenerator {
    create_integration_test_tx_generator()
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

type CreateRpcTxsFn = fn(&mut MultiAccountTransactionGenerator) -> Vec<RpcTransaction>;
type TestTxHashesFn = fn(&[TransactionHash]) -> Vec<TransactionHash>;
type ExpectedContentId = Felt;

fn create_test_blocks() -> Vec<(BlockNumber, CreateRpcTxsFn, TestTxHashesFn, ExpectedContentId)> {
    let next_height = INITIAL_HEIGHT.unchecked_next();
    let heights_to_build = next_height.iter_up_to(LAST_HEIGHT.unchecked_next());
    let test_scenarios: Vec<(CreateRpcTxsFn, TestTxHashesFn, ExpectedContentId)> = vec![
        (
            create_multiple_account_txs,
            test_multiple_account_txs,
            Felt::from_hex_unchecked(
                "0x665101f416fd5c4e91083fa9dcac1dba9a282f5211a1a2ad7695e95cb35d6b",
            ),
        ),
        (
            create_funding_txs,
            test_single_tx,
            Felt::from_hex_unchecked(
                "0x354a08374de0b194773930010006a0cc42f7f984f509ceb0d564da37ed15bab",
            ),
        ),
        (
            deploy_account,
            test_single_tx,
            Felt::from_hex_unchecked(
                "0x2942454db8523de50045d2cc28f9fe9342c56f1c07af35d6bdd5ba1f68700b6",
            ),
        ),
        // Note: The following test scenario sends 15 transactions but only 12 are included in the
        // block. This means that the last 3 transactions could be included in the next block if
        // one is added to the test.
        (
            create_many_invoke_txs,
            test_many_invoke_txs,
            Felt::from_hex_unchecked(
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
    expected_content_id: Felt,
    expected_proposer_id: ValidatorId,
) {
    let chain_id = CHAIN_ID_FOR_TESTS.clone();
    let broadcasted_messages_receiver =
        &mut consensus_proposals_channels.broadcasted_messages_receiver;
    // TODO (Dan, Guy): retrieve / calculate the expected proposal init and fin.
    let expected_proposal_init = ProposalInit {
        height: expected_height,
        proposer: expected_proposer_id,
        ..Default::default()
    };
    let expected_proposal_fin = ProposalFin { proposal_content_id: BlockHash(expected_content_id) };

    let StreamMessage {
        stream_id: first_stream_id,
        message: init_message,
        message_id: incoming_message_id,
    } = broadcasted_messages_receiver.next().await.unwrap().0.unwrap();

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
    loop {
        let StreamMessage { message, stream_id, message_id: _ } =
            broadcasted_messages_receiver.next().await.unwrap().0.unwrap();
        assert_eq!(stream_id, first_stream_id, "Expected the same stream id for all messages");
        match message {
            StreamMessageBody::Content(ProposalPart::Init(init)) => {
                panic!("Unexpected init: {:?}", init)
            }
            StreamMessageBody::Content(ProposalPart::Transactions(transactions)) => {
                received_tx_hashes.extend(
                    transactions
                        .transactions
                        .iter()
                        .map(|tx| tx.calculate_transaction_hash(&chain_id).unwrap()),
                );
            }
            StreamMessageBody::Content(ProposalPart::Fin(proposal_fin)) => {
                assert_eq!(
                    proposal_fin, expected_proposal_fin,
                    "Unexpected fin message: {:?}, expected: {:?}",
                    proposal_fin, expected_proposal_fin
                );
                got_proposal_fin = true;
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

fn deploy_account(tx_generator: &mut MultiAccountTransactionGenerator) -> Vec<RpcTransaction> {
    let undeployed_account_tx_generator = tx_generator.account_with_id_mut(UNDEPLOYED_ACCOUNT_ID);
    assert!(!undeployed_account_tx_generator.is_deployed());
    let deploy_tx = undeployed_account_tx_generator.generate_deploy_account();
    vec![deploy_tx]
}

fn test_single_tx(tx_hashes: &[TransactionHash]) -> Vec<TransactionHash> {
    assert_eq!(tx_hashes.len(), 1, "Expected a single transaction");
    tx_hashes.to_vec()
}
