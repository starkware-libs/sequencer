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
use starknet_integration_tests::flow_test_setup::FlowTestSetup;
use starknet_integration_tests::utils::{
    create_integration_test_tx_generator,
    run_integration_test_scenario,
};
use starknet_types_core::felt::Felt;

#[fixture]
fn tx_generator() -> MultiAccountTransactionGenerator {
    create_integration_test_tx_generator()
}

#[rstest]
#[tokio::test]
async fn end_to_end(tx_generator: MultiAccountTransactionGenerator) {
    const LISTEN_TO_BROADCAST_MESSAGES_TIMEOUT: std::time::Duration =
        std::time::Duration::from_secs(5);
    // Setup.
    let mut mock_running_system = FlowTestSetup::new_from_tx_generator(&tx_generator).await;

    // Create and send transactions.
    let expected_batched_tx_hashes = run_integration_test_scenario(tx_generator, &mut |tx| {
        mock_running_system.assert_add_tx_success(tx)
    })
    .await;
    // TODO(Dan, Itay): Consider adding a utility function that waits for something to happen.
    tokio::time::timeout(
        LISTEN_TO_BROADCAST_MESSAGES_TIMEOUT,
        listen_to_broadcasted_messages(
            &mut mock_running_system.consensus_proposals_channels,
            &expected_batched_tx_hashes,
        ),
    )
    .await
    .expect("listen to broadcasted messages should finish in time");
}

async fn listen_to_broadcasted_messages(
    consensus_proposals_channels: &mut BroadcastTopicChannels<StreamMessage<ProposalPart>>,
    expected_batched_tx_hashes: &[TransactionHash],
) {
    let chain_id = CHAIN_ID_FOR_TESTS.clone();
    let broadcasted_messages_receiver =
        &mut consensus_proposals_channels.broadcasted_messages_receiver;
    // TODO (Dan, Guy): retrieve / calculate the expected proposal init and fin.
    let expected_proposal_init = ProposalInit {
        height: BlockNumber(1),
        round: 0,
        valid_round: None,
        proposer: ValidatorId::from(1991_u32),
    };
    let expected_proposal_fin = ProposalFin {
        proposal_content_id: BlockHash(Felt::from_hex_unchecked(
            "0x4597ceedbef644865917bf723184538ef70d43954d63f5b7d8cb9d1bd4c2c32",
        )),
    };

    let StreamMessage {
        stream_id: first_stream_id,
        message: init_message,
        message_id: incoming_message_id,
    } = broadcasted_messages_receiver.next().await.unwrap().0.unwrap();

    assert_eq!(incoming_message_id, 0);
    let StreamMessageBody::Content(ProposalPart::Init(incoming_proposal_init)) = init_message
    else {
        panic!("Expected an init message. Got: {:?}", init_message)
    };
    assert_eq!(incoming_proposal_init, expected_proposal_init);

    let mut received_tx_hashes = HashSet::new();
    let mut got_proposal_fin = false;
    let mut got_channel_fin = false;
    loop {
        let StreamMessage { message, stream_id, message_id: _ } =
            broadcasted_messages_receiver.next().await.unwrap().0.unwrap();
        assert_eq!(stream_id, first_stream_id);
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
                assert_eq!(proposal_fin, expected_proposal_fin);
                got_proposal_fin = true;
            }
            StreamMessageBody::Fin => {
                got_channel_fin = true;
            }
        }
        if got_proposal_fin
            && got_channel_fin
            && received_tx_hashes.len() == expected_batched_tx_hashes.len()
        {
            break;
        }
    }

    // Using HashSet to ignore the order of the transactions (broadcast can lead to reordering).
    assert_eq!(
        received_tx_hashes,
        expected_batched_tx_hashes.iter().cloned().collect::<HashSet<_>>()
    );
}
