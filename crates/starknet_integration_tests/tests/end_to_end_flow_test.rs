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
use tracing::debug;

const INITIAL_HEIGHT: BlockNumber = BlockNumber(0);
const LAST_HEIGHT: BlockNumber = BlockNumber(2);

#[fixture]
fn tx_generator() -> MultiAccountTransactionGenerator {
    create_integration_test_tx_generator()
}

#[rstest]
#[tokio::test]
async fn end_to_end(mut tx_generator: MultiAccountTransactionGenerator) {
    const LISTEN_TO_BROADCAST_MESSAGES_TIMEOUT: std::time::Duration =
        std::time::Duration::from_secs(5);
    // Setup.
    let mut mock_running_system = FlowTestSetup::new_from_tx_generator(&tx_generator).await;

    let next_height = INITIAL_HEIGHT.unchecked_next();
    let heights_to_build = next_height.iter_up_to(LAST_HEIGHT.unchecked_next());
    let expected_content_ids = [
        Felt::from_hex_unchecked(
            "0x7d62e32fd8f1a12104a5d215af26ec0f362da81af3d14c24e08e46976cdfbf5",
        ),
        Felt::from_hex_unchecked(
            "0x259aeaad847bffe6c342998c4510e5e474577219cfbb118f5cb2f2286260d52",
        ),
    ];

    // Buld multiple heights to ensure heights are committed.
    for (height, expected_content_id) in itertools::zip_eq(heights_to_build, expected_content_ids) {
        debug!("Starting height {}.", height);
        // Create and send transactions.
        let expected_batched_tx_hashes =
            run_integration_test_scenario(&mut tx_generator, &mut |tx| {
                mock_running_system.assert_add_tx_success(tx)
            })
            .await;
        // TODO(Dan, Itay): Consider adding a utility function that waits for something to happen.
        tokio::time::timeout(
            LISTEN_TO_BROADCAST_MESSAGES_TIMEOUT,
            listen_to_broadcasted_messages(
                &mut mock_running_system.consensus_proposals_channels,
                &expected_batched_tx_hashes,
                height,
                expected_content_id,
            ),
        )
        .await
        .expect("listen to broadcasted messages should finish in time");
    }
}

async fn listen_to_broadcasted_messages(
    consensus_proposals_channels: &mut BroadcastTopicChannels<StreamMessage<ProposalPart>>,
    expected_batched_tx_hashes: &[TransactionHash],
    expected_height: BlockNumber,
    expected_content_id: Felt,
) {
    let chain_id = CHAIN_ID_FOR_TESTS.clone();
    let broadcasted_messages_receiver =
        &mut consensus_proposals_channels.broadcasted_messages_receiver;
    // TODO (Dan, Guy): retrieve / calculate the expected proposal init and fin.
    let expected_proposal_init = ProposalInit {
        height: expected_height,
        round: 0,
        valid_round: None,
        proposer: ValidatorId::from(100_u32),
    };
    let expected_proposal_fin = ProposalFin { proposal_content_id: BlockHash(expected_content_id) };

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
