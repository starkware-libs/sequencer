use futures::channel::{mpsc, oneshot};
use futures::StreamExt;
use papyrus_consensus::types::{ConsensusContext, ProposalInit};
use papyrus_network::network_manager::test_utils::{
    mock_register_broadcast_topic,
    BroadcastNetworkMock,
};
use papyrus_protobuf::consensus::{ConsensusMessage, Proposal, Vote};
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

    let (mut proposal_receiver, fin_receiver) = papyrus_context.build_proposal(block_number).await;

    let mut transactions = Vec::new();
    while let Some(tx) = proposal_receiver.next().await {
        transactions.push(tx);
    }
    assert_eq!(transactions, block.body.transactions);

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

    let fin =
        papyrus_context.validate_proposal(block_number, validate_receiver).await.await.unwrap();

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

    let fin = papyrus_context.validate_proposal(block_number, validate_receiver).await.await;
    assert_eq!(fin, Err(oneshot::Canceled));
}

#[tokio::test]
async fn propose() {
    let (block, papyrus_context, mut mock_network, _) = test_setup();
    let block_number = block.header.block_header_without_hash.block_number;

    let (mut content_sender, content_receiver) = mpsc::channel(TEST_CHANNEL_SIZE);
    for tx in block.body.transactions.clone() {
        content_sender.try_send(tx).unwrap();
    }
    content_sender.close_channel();

    let (fin_sender, fin_receiver) = oneshot::channel();
    fin_sender.send(block.header.block_hash).unwrap();

    let proposal_init = ProposalInit {
        height: block_number,
        round: 0,
        proposer: ContractAddress::default(),
        valid_round: None,
    };
    papyrus_context.propose(proposal_init.clone(), content_receiver, fin_receiver).await.unwrap();

    let expected_message = ConsensusMessage::Proposal(Proposal {
        height: proposal_init.height.0,
        round: 0,
        proposer: proposal_init.proposer,
        transactions: block.body.transactions,
        block_hash: block.header.block_hash,
        valid_round: None,
    });

    assert_eq!(mock_network.messages_to_broadcast_receiver.next().await.unwrap(), expected_message);
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
    let sync_channels = mock_register_broadcast_topic().unwrap();
    let papyrus_context = PapyrusConsensusContext::new(
        storage_reader.clone(),
        network_channels.subscriber_channels.broadcast_topic_client,
        4,
        Some(sync_channels.subscriber_channels.broadcast_topic_client),
    );
    (block, papyrus_context, network_channels.mock_network, sync_channels.mock_network)
}
