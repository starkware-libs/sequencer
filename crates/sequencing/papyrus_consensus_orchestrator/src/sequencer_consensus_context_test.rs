use std::sync::{Arc, OnceLock};
use std::time::Duration;
use std::vec;

use futures::channel::mpsc;
use futures::SinkExt;
use lazy_static::lazy_static;
use papyrus_consensus::stream_handler::StreamHandler;
use papyrus_consensus::types::ConsensusContext;
use papyrus_network::network_manager::test_utils::{
    mock_register_broadcast_topic,
    TestSubscriberChannels,
};
use papyrus_network::network_manager::BroadcastTopicChannels;
use papyrus_protobuf::consensus::ProposalInit;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{ContractAddress, StateDiffCommitment};
use starknet_api::executable_transaction::{AccountTransaction, Transaction};
use starknet_api::hash::PoseidonHash;
use starknet_api::test_utils::invoke::{executable_invoke_tx, InvokeTxArgs};
use starknet_api::transaction::TransactionHash;
use starknet_batcher_types::batcher_types::{
    GetProposalContent,
    GetProposalContentResponse,
    ProposalCommitment,
    ProposalId,
    ProposalStatus,
    ProposeBlockInput,
    SendProposalContent,
    SendProposalContentInput,
    SendProposalContentResponse,
    StartHeightInput,
    ValidateBlockInput,
};
use starknet_batcher_types::communication::MockBatcherClient;
use starknet_types_core::felt::Felt;

use crate::sequencer_consensus_context::SequencerConsensusContext;

const TIMEOUT: Duration = Duration::from_millis(100);
const CHANNEL_SIZE: usize = 5000;
const NUM_VALIDATORS: u64 = 4;
const STATE_DIFF_COMMITMENT: StateDiffCommitment = StateDiffCommitment(PoseidonHash(Felt::ZERO));

lazy_static! {
    static ref TX_BATCH: Vec<Transaction> = vec![generate_invoke_tx(Felt::THREE)];
}

fn generate_invoke_tx(tx_hash: Felt) -> Transaction {
    Transaction::Account(AccountTransaction::Invoke(executable_invoke_tx(InvokeTxArgs {
        tx_hash: TransactionHash(tx_hash),
        ..Default::default()
    })))
}

#[tokio::test]
async fn build_proposal() {
    let mut batcher = MockBatcherClient::new();
    let proposal_id = Arc::new(OnceLock::new());
    let proposal_id_clone = Arc::clone(&proposal_id);
    batcher.expect_propose_block().returning(move |input: ProposeBlockInput| {
        proposal_id_clone.set(input.proposal_id).unwrap();
        Ok(())
    });
    batcher.expect_start_height().return_once(|input: StartHeightInput| {
        assert_eq!(input.height, BlockNumber(0));
        Ok(())
    });
    let proposal_id_clone = Arc::clone(&proposal_id);
    batcher.expect_get_proposal_content().times(1).returning(move |input| {
        assert_eq!(input.proposal_id, *proposal_id_clone.get().unwrap());
        Ok(GetProposalContentResponse { content: GetProposalContent::Txs(TX_BATCH.clone()) })
    });
    let proposal_id_clone = Arc::clone(&proposal_id);
    batcher.expect_get_proposal_content().times(1).returning(move |input| {
        assert_eq!(input.proposal_id, *proposal_id_clone.get().unwrap());
        Ok(GetProposalContentResponse {
            content: GetProposalContent::Finished(ProposalCommitment {
                state_diff_commitment: STATE_DIFF_COMMITMENT,
            }),
        })
    });
    // TODO(guyn): remove this first set of channels once we are using only the streaming channels.
    let TestSubscriberChannels { mock_network: _mock_network, subscriber_channels } =
        mock_register_broadcast_topic().expect("Failed to create mock network");
    let BroadcastTopicChannels { broadcasted_messages_receiver: _, broadcast_topic_client } =
        subscriber_channels;

    let TestSubscriberChannels { mock_network: _mock_network, subscriber_channels } =
        mock_register_broadcast_topic().expect("Failed to create mock network");
    let BroadcastTopicChannels {
        broadcasted_messages_receiver: inbound_network_receiver,
        broadcast_topic_client: outbound_network_sender,
    } = subscriber_channels;
    let (outbound_internal_sender, _inbound_internal_receiver, _) =
        StreamHandler::get_channels(inbound_network_receiver, outbound_network_sender);

    let mut context = SequencerConsensusContext::new(
        Arc::new(batcher),
        broadcast_topic_client,
        outbound_internal_sender,
        NUM_VALIDATORS,
    );
    let init = ProposalInit {
        height: BlockNumber(0),
        round: 0,
        proposer: ContractAddress::default(),
        valid_round: None,
    };
    // TODO(Asmaa): Test proposal content.
    let fin_receiver = context.build_proposal(init, TIMEOUT).await;
    assert_eq!(fin_receiver.await.unwrap().0, STATE_DIFF_COMMITMENT.0.0);
}

#[tokio::test]
async fn validate_proposal_success() {
    let mut batcher = MockBatcherClient::new();
    let proposal_id: Arc<OnceLock<ProposalId>> = Arc::new(OnceLock::new());
    let proposal_id_clone = Arc::clone(&proposal_id);
    batcher.expect_validate_block().returning(move |input: ValidateBlockInput| {
        proposal_id_clone.set(input.proposal_id).unwrap();
        Ok(())
    });
    batcher.expect_start_height().return_once(|input: StartHeightInput| {
        assert_eq!(input.height, BlockNumber(0));
        Ok(())
    });
    let proposal_id_clone = Arc::clone(&proposal_id);
    batcher.expect_send_proposal_content().times(1).returning(
        move |input: SendProposalContentInput| {
            assert_eq!(input.proposal_id, *proposal_id_clone.get().unwrap());
            let SendProposalContent::Txs(txs) = input.content else {
                panic!("Expected SendProposalContent::Txs, got {:?}", input.content);
            };
            assert_eq!(txs, TX_BATCH.clone());
            Ok(SendProposalContentResponse { response: ProposalStatus::Processing })
        },
    );
    let proposal_id_clone = Arc::clone(&proposal_id);
    batcher.expect_send_proposal_content().times(1).returning(
        move |input: SendProposalContentInput| {
            assert_eq!(input.proposal_id, *proposal_id_clone.get().unwrap());
            assert!(matches!(input.content, SendProposalContent::Finish));
            Ok(SendProposalContentResponse {
                response: ProposalStatus::Finished(ProposalCommitment {
                    state_diff_commitment: STATE_DIFF_COMMITMENT,
                }),
            })
        },
    );
    // TODO(guyn): remove this first set of channels once we are using only the streaming channels.
    let TestSubscriberChannels { mock_network: _, subscriber_channels } =
        mock_register_broadcast_topic().expect("Failed to create mock network");
    let BroadcastTopicChannels { broadcasted_messages_receiver: _, broadcast_topic_client } =
        subscriber_channels;

    let TestSubscriberChannels { mock_network: _mock_network, subscriber_channels } =
        mock_register_broadcast_topic().expect("Failed to create mock network");
    let BroadcastTopicChannels {
        broadcasted_messages_receiver: inbound_network_receiver,
        broadcast_topic_client: outbound_network_sender,
    } = subscriber_channels;
    let (outbound_internal_sender, _inbound_internal_receiver, _) =
        StreamHandler::get_channels(inbound_network_receiver, outbound_network_sender);

    let mut context = SequencerConsensusContext::new(
        Arc::new(batcher),
        broadcast_topic_client,
        outbound_internal_sender,
        NUM_VALIDATORS,
    );
    let (mut content_sender, content_receiver) = mpsc::channel(CHANNEL_SIZE);
    content_sender.send(TX_BATCH.clone()).await.unwrap();
    let fin_receiver = context.validate_proposal(BlockNumber(0), TIMEOUT, content_receiver).await;
    content_sender.close_channel();
    assert_eq!(fin_receiver.await.unwrap().0, STATE_DIFF_COMMITMENT.0.0);
}

#[tokio::test]
async fn repropose() {
    // Receive a proposal. Then re-retrieve it.
    let mut batcher = MockBatcherClient::new();
    batcher.expect_validate_block().returning(move |_| Ok(()));
    batcher.expect_start_height().return_once(|input: StartHeightInput| {
        assert_eq!(input.height, BlockNumber(0));
        Ok(())
    });
    batcher.expect_send_proposal_content().times(1).returning(
        move |input: SendProposalContentInput| {
            assert!(matches!(input.content, SendProposalContent::Txs(_)));
            Ok(SendProposalContentResponse { response: ProposalStatus::Processing })
        },
    );
    batcher.expect_send_proposal_content().times(1).returning(
        move |input: SendProposalContentInput| {
            assert!(matches!(input.content, SendProposalContent::Finish));
            Ok(SendProposalContentResponse {
                response: ProposalStatus::Finished(ProposalCommitment {
                    state_diff_commitment: STATE_DIFF_COMMITMENT,
                }),
            })
        },
    );
    // TODO(guyn): remove this first set of channels once we are using only the streaming channels.
    let TestSubscriberChannels { mock_network: _, subscriber_channels } =
        mock_register_broadcast_topic().expect("Failed to create mock network");
    let BroadcastTopicChannels { broadcasted_messages_receiver: _, broadcast_topic_client } =
        subscriber_channels;

    let TestSubscriberChannels { mock_network: _mock_network, subscriber_channels } =
        mock_register_broadcast_topic().expect("Failed to create mock network");
    let BroadcastTopicChannels {
        broadcasted_messages_receiver: inbound_network_receiver,
        broadcast_topic_client: outbound_network_sender,
    } = subscriber_channels;
<<<<<<< HEAD
    let (outbound_internal_sender, _inbound_internal_receiver, _) =
=======
    let (outbound_internal_sender, _inbound_internal_receiver) =
>>>>>>> 883a253be (feat: allow a streamed proposal channel on top of existing one)
        StreamHandler::get_channels(inbound_network_receiver, outbound_network_sender);

    let mut context = SequencerConsensusContext::new(
        Arc::new(batcher),
        broadcast_topic_client,
        outbound_internal_sender,
        NUM_VALIDATORS,
    );

    // Receive a valid proposal.
    let (mut content_sender, content_receiver) = mpsc::channel(CHANNEL_SIZE);
    let txs = vec![generate_invoke_tx(Felt::TWO)];
    content_sender.send(txs.clone()).await.unwrap();
    let fin_receiver = context.validate_proposal(BlockNumber(0), TIMEOUT, content_receiver).await;
    content_sender.close_channel();
    assert_eq!(fin_receiver.await.unwrap().0, STATE_DIFF_COMMITMENT.0.0);

    // Re-proposal: Just asserts this is a known valid proposal.
    context
        .repropose(
            BlockHash(STATE_DIFF_COMMITMENT.0.0),
            ProposalInit { height: BlockNumber(0), ..Default::default() },
        )
        .await;
}
