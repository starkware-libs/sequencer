use std::collections::HashSet;

use futures::StreamExt;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use papyrus_consensus::types::ValidatorId;
use papyrus_network::network_manager::BroadcastTopicChannels;
use papyrus_protobuf::consensus::{
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
use starknet_api::transaction::TransactionHash;
use starknet_integration_tests::flow_test_setup::{FlowSequencerSetup, FlowTestSetup};
use starknet_integration_tests::test_identifiers::TestIdentifier;
use starknet_integration_tests::utils::{
    create_integration_test_tx_generator,
    run_integration_test_scenario,
};
use starknet_sequencer_infra::trace_util::configure_tracing;
use starknet_types_core::felt::Felt;
use tracing::debug;

const INITIAL_HEIGHT: BlockNumber = BlockNumber(0);
const LAST_HEIGHT: BlockNumber = BlockNumber(2);

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

    let next_height = INITIAL_HEIGHT.unchecked_next();
    let heights_to_build = next_height.iter_up_to(LAST_HEIGHT.unchecked_next());
    let expected_content_ids = [
        Felt::from_hex_unchecked(
            "0x58ad05a6987a675eda038663d8e7dcc8e1d91c9057dd57f16d9b3b9602fc840",
        ),
        Felt::from_hex_unchecked(
            "0x79b59c5036c9427b5194796ede67bdfffed1f311a77382d715174fcfcc33003",
        ),
    ];

    let sequencers = [&mock_running_system.sequencer_0, &mock_running_system.sequencer_1];
    // We use only the first sequencer's gateway to test that the mempools are syncing.
    let sequencer_to_add_txs = *sequencers.first().unwrap();
    let mut expected_proposer_iter = sequencers.iter().cycle();
    // We start at height 1, so we need to skip the proposer of the initial height.
    expected_proposer_iter.next().unwrap();

    // Build multiple heights to ensure heights are committed.
    for (height, expected_content_id) in itertools::zip_eq(heights_to_build, expected_content_ids) {
        debug!("Starting height {}.", height);
        // Create and send transactions.
        let expected_batched_tx_hashes =
            run_integration_test_scenario(&mut tx_generator, &mut |tx| {
                sequencer_to_add_txs.assert_add_tx_success(tx)
            })
            .await;
        let expected_validator_id = expected_proposer_iter
            .next()
            .unwrap()
            .config
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

async fn wait_for_sequencer_node(sequencer: &FlowSequencerSetup) {
    sequencer.is_alive_test_client.await_alive(5000, 50).await.expect("Node should be alive.");
}

async fn listen_to_broadcasted_messages(
    consensus_proposals_channels: &mut BroadcastTopicChannels<StreamMessage<ProposalPart>>,
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

    let mut received_tx_hashes = HashSet::new();
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

    // Using HashSet to ignore the order of the transactions (broadcast can lead to reordering).
    assert_eq!(
        received_tx_hashes,
        expected_batched_tx_hashes.iter().cloned().collect::<HashSet<_>>(),
        "Unexpected transactions"
    );
}
