use std::collections::HashSet;

use futures::StreamExt;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
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
use starknet_api::core::ContractAddress;
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
    let mut received_tx_hashes = HashSet::new();
    // TODO (Dan, Guy): retrieve / calculate the expected proposal init and fin.
    let expected_proposal_init = ProposalInit {
        height: BlockNumber(1),
        round: 0,
        valid_round: None,
        proposer: ContractAddress::default(),
    };
    let expected_proposal_fin = ProposalFin {
        proposal_content_id: BlockHash(Felt::from_hex_unchecked(
            "0x4597ceedbef644865917bf723184538ef70d43954d63f5b7d8cb9d1bd4c2c32",
        )),
    };

    let incoming_message = broadcasted_messages_receiver.next().await.unwrap().0.unwrap();
    let incoming_stream_id = incoming_message.stream_id;
    assert_eq!(incoming_message.message_id, 0);
    let incoming_message = incoming_message.message;
    let StreamMessageBody::Content(ProposalPart::Init(received_proposal_init)) = incoming_message
    else {
        panic!("Unexpected init: {:?}", incoming_message);
    };
    assert_eq!(received_proposal_init, expected_proposal_init);

    let mut proposal_parts_fin = false;
    let mut message_body_fin = false;
    loop {
        let message = broadcasted_messages_receiver.next().await.unwrap().0.unwrap();
        assert_eq!(message.stream_id, incoming_stream_id);
        match message.message {
            StreamMessageBody::Content(ProposalPart::Init(init)) => {
                panic!("Unexpected init: {:?}", init)
            }
            StreamMessageBody::Content(ProposalPart::Fin(proposal_fin)) => {
                assert_eq!(proposal_fin, expected_proposal_fin);
                proposal_parts_fin = true;
            }
            StreamMessageBody::Content(ProposalPart::Transactions(transactions)) => {
                received_tx_hashes.extend(
                    transactions
                        .transactions
                        .iter()
                        .map(|tx| tx.calculate_transaction_hash(&chain_id).unwrap()),
                );
            }
            // Ignore this, in case it comes out of the network before some of the other messages.
            StreamMessageBody::Fin => {
                message_body_fin = true;
            }
        }
        if proposal_parts_fin && message_body_fin {
            break;
        }
    }
    // Using HashSet to ignore the order of the transactions (broadcast can lead to reordering).
    assert_eq!(
        received_tx_hashes,
        expected_batched_tx_hashes.iter().cloned().collect::<HashSet<_>>()
    );
}
