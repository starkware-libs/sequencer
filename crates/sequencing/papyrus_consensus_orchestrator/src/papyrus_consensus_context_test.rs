use std::time::Duration;

use futures::channel::{mpsc, oneshot};
use futures::StreamExt;
use papyrus_consensus::stream_handler::StreamHandler;
use papyrus_consensus::types::ConsensusContext;
use papyrus_network::network_manager::test_utils::{
    mock_register_broadcast_topic,
    BroadcastNetworkMock,
    TestSubscriberChannels,
};
use papyrus_network::network_manager::BroadcastTopicChannels;
use papyrus_protobuf::consensus::{
    ConsensusMessage,
    ProposalInit,
    ProposalPart,
    StreamMessage,
    Vote,
};
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::test_utils::get_test_storage;
use papyrus_test_utils::get_test_block;
use starknet_api::block::{Block, BlockHash};
use starknet_api::core::ContractAddress;

use crate::papyrus_consensus_context::PapyrusConsensusContext;

// TODO(dvir): consider adding tests for times, i.e, the calls are returned immediately and nothing
// happen until it should (for example, not creating a block before we have it in storage).

const TEST_CHANNEL_SIZE: usize = 10;

#[tokio::test]
async fn build_proposal() {
    let (block, mut papyrus_context, _mock_network, _) = test_setup();
    let block_number = block.header.block_header_without_hash.block_number;
    let proposal_init = ProposalInit {
        height: block_number,
        round: 0,
        proposer: ContractAddress::default(),
        valid_round: None,
    };
    // TODO(Asmaa): Test proposal content.
    let fin_receiver = papyrus_context.build_proposal(proposal_init, Duration::MAX).await;

    let fin = fin_receiver.await.unwrap();
    assert_eq!(fin, block.header.block_hash);
}

#[tokio::test]
async fn validate_proposal_success() {
    let (block, mut papyrus_context, _mock_network, _) = test_setup();
    let block_number = block.header.block_header_without_hash.block_number;

    let (mut validate_sender, validate_receiver) = mpsc::channel(TEST_CHANNEL_SIZE);
    for tx in block.body.transactions.clone() {
        validate_sender.try_send(tx).unwrap();
    }
    validate_sender.close_channel();

    let fin = papyrus_context
        .validate_proposal(block_number, Duration::MAX, validate_receiver)
        .await
        .await
        .unwrap();

    assert_eq!(fin, block.header.block_hash);
}

#[tokio::test]
async fn validate_proposal_fail() {
    let (block, mut papyrus_context, _mock_network, _) = test_setup();
    let block_number = block.header.block_header_without_hash.block_number;

    let different_block = get_test_block(4, None, None, None);
    let (mut validate_sender, validate_receiver) = mpsc::channel(5000);
    for tx in different_block.body.transactions.clone() {
        validate_sender.try_send(tx).unwrap();
    }
    validate_sender.close_channel();

    let fin = papyrus_context
        .validate_proposal(block_number, Duration::MAX, validate_receiver)
        .await
        .await;
    assert_eq!(fin, Err(oneshot::Canceled));
}

#[tokio::test]
async fn decision() {
    let (_, mut papyrus_context, _, mut sync_network) = test_setup();
    let block = BlockHash::default();
    let precommit = Vote::default();
    papyrus_context.decision_reached(block, vec![precommit.clone()]).await.unwrap();
    assert_eq!(sync_network.messages_to_broadcast_receiver.next().await.unwrap(), precommit);
}

fn test_setup() -> (
    Block,
    PapyrusConsensusContext,
    BroadcastNetworkMock<ConsensusMessage>,
    BroadcastNetworkMock<Vote>,
) {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let block = get_test_block(5, None, None, None);
    let block_number = block.header.block_header_without_hash.block_number;
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block_number, &block.header)
        .unwrap()
        .append_body(block_number, block.body.clone())
        .unwrap()
        .commit()
        .unwrap();

    let network_channels = mock_register_broadcast_topic().unwrap();
    let network_proposal_channels: TestSubscriberChannels<StreamMessage<ProposalPart>> =
        mock_register_broadcast_topic().unwrap();
    let BroadcastTopicChannels {
        broadcasted_messages_receiver: inbound_network_receiver,
        broadcast_topic_client: outbound_network_sender,
    } = network_proposal_channels.subscriber_channels;
    let (outbound_internal_sender, _inbound_internal_receiver, _) =
        StreamHandler::get_channels(inbound_network_receiver, outbound_network_sender);

    let sync_channels = mock_register_broadcast_topic().unwrap();

    let papyrus_context = PapyrusConsensusContext::new(
        storage_reader.clone(),
        network_channels.subscriber_channels.broadcast_topic_client,
        outbound_internal_sender,
        4,
        Some(sync_channels.subscriber_channels.broadcast_topic_client),
    );
    (block, papyrus_context, network_channels.mock_network, sync_channels.mock_network)
}
