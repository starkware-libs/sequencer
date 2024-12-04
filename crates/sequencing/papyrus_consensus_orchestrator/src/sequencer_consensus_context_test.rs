use std::sync::{Arc, OnceLock};
use std::time::Duration;
use std::vec;

use futures::channel::mpsc;
use futures::{FutureExt, SinkExt};
use lazy_static::lazy_static;
use papyrus_consensus::stream_handler::StreamHandler;
use papyrus_consensus::types::{ConsensusContext, ValidatorId};
use papyrus_network::network_manager::test_utils::{
    mock_register_broadcast_topic,
    BroadcastNetworkMock,
    TestSubscriberChannels,
};
use papyrus_network::network_manager::BroadcastTopicChannels;
use papyrus_protobuf::consensus::{
    ConsensusMessage,
    ProposalFin,
    ProposalInit,
    ProposalPart,
    StreamMessage,
    TransactionBatch,
};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::StateDiffCommitment;
use starknet_api::executable_transaction::{
    AccountTransaction,
    Transaction as ExecutableTransaction,
};
use starknet_api::hash::PoseidonHash;
use starknet_api::test_utils::invoke::{executable_invoke_tx, invoke_tx, InvokeTxArgs};
use starknet_api::transaction::{Transaction, TransactionHash};
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
    static ref TX_BATCH: Vec<ExecutableTransaction> =
        vec![generate_executable_invoke_tx(Felt::THREE)];
}

fn generate_invoke_tx() -> Transaction {
    Transaction::Invoke(invoke_tx(InvokeTxArgs::default()))
}

fn generate_executable_invoke_tx(tx_hash: Felt) -> ExecutableTransaction {
    ExecutableTransaction::Account(AccountTransaction::Invoke(executable_invoke_tx(InvokeTxArgs {
        tx_hash: TransactionHash(tx_hash),
        ..Default::default()
    })))
}

// Structs which aren't utilized but should not be dropped.
struct NetworkDependencies {
    _vote_network: BroadcastNetworkMock<ConsensusMessage>,
    _new_proposal_network: BroadcastNetworkMock<StreamMessage<ProposalPart>>,
}

fn setup(batcher: MockBatcherClient) -> (SequencerConsensusContext, NetworkDependencies) {
    let TestSubscriberChannels { mock_network: mock_proposal_stream_network, subscriber_channels } =
        mock_register_broadcast_topic().expect("Failed to create mock network");
    let BroadcastTopicChannels {
        broadcasted_messages_receiver: inbound_network_receiver,
        broadcast_topic_client: outbound_network_sender,
    } = subscriber_channels;
    let (outbound_proposal_stream_sender, _, _) =
        StreamHandler::get_channels(inbound_network_receiver, outbound_network_sender);

    let TestSubscriberChannels { mock_network: mock_vote_network, subscriber_channels } =
        mock_register_broadcast_topic().expect("Failed to create mock network");
    let BroadcastTopicChannels { broadcast_topic_client: votes_topic_client, .. } =
        subscriber_channels;

    let context = SequencerConsensusContext::new(
        Arc::new(batcher),
        outbound_proposal_stream_sender,
        votes_topic_client,
        NUM_VALIDATORS,
    );

    let network_dependencies = NetworkDependencies {
        _vote_network: mock_vote_network,
        _new_proposal_network: mock_proposal_stream_network,
    };

    (context, network_dependencies)
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
    batcher
        .expect_start_height()
        .withf(|input| input.height == BlockNumber(0))
        .return_once(|_| Ok(()));
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
    let (mut context, _network) = setup(batcher);

    let init = ProposalInit {
        height: BlockNumber(0),
        round: 0,
        proposer: ValidatorId::default(),
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
    batcher
        .expect_start_height()
        .withf(|input| input.height == BlockNumber(0))
        .return_once(|_| Ok(()));
    let proposal_id_clone = Arc::clone(&proposal_id);
    batcher.expect_send_proposal_content().times(1).returning(
        move |input: SendProposalContentInput| {
            assert_eq!(input.proposal_id, *proposal_id_clone.get().unwrap());
            let SendProposalContent::Txs(txs) = input.content else {
                panic!("Expected SendProposalContent::Txs, got {:?}", input.content);
            };
            assert_eq!(txs, *TX_BATCH);
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
    let (mut context, _network) = setup(batcher);

    // Initialize the context for a specific height, starting with round 0.
    context.set_height_and_round(BlockNumber(0), 0).await;

    let (mut content_sender, content_receiver) = mpsc::channel(CHANNEL_SIZE);
    let tx_hash = TX_BATCH.first().unwrap().tx_hash();
    let txs =
        TX_BATCH.clone().into_iter().map(starknet_api::transaction::Transaction::from).collect();
    content_sender
        .send(ProposalPart::Transactions(TransactionBatch {
            transactions: txs,
            tx_hashes: vec![tx_hash],
        }))
        .await
        .unwrap();
    content_sender
        .send(ProposalPart::Fin(ProposalFin {
            proposal_content_id: BlockHash(STATE_DIFF_COMMITMENT.0.0),
        }))
        .await
        .unwrap();
    let fin_receiver = context
        .validate_proposal(BlockNumber(0), 0, ValidatorId::default(), TIMEOUT, content_receiver)
        .await;
    content_sender.close_channel();
    assert_eq!(fin_receiver.await.unwrap().0.0, STATE_DIFF_COMMITMENT.0.0);
}

#[tokio::test]
async fn repropose() {
    // Receive a proposal. Then re-retrieve it.
    let mut batcher = MockBatcherClient::new();
    batcher.expect_validate_block().returning(move |_| Ok(()));
    batcher
        .expect_start_height()
        .withf(|input| input.height == BlockNumber(0))
        .return_once(|_| Ok(()));
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
    let (mut context, _network) = setup(batcher);

    // Initialize the context for a specific height, starting with round 0.
    context.set_height_and_round(BlockNumber(0), 0).await;

    // Receive a valid proposal.
    let (mut content_sender, content_receiver) = mpsc::channel(CHANNEL_SIZE);
    let prop_part = ProposalPart::Transactions(TransactionBatch {
        transactions: vec![generate_invoke_tx()],
        tx_hashes: vec![TransactionHash(Felt::TWO)],
    });
    content_sender.send(prop_part).await.unwrap();
    let prop_part = ProposalPart::Fin(ProposalFin {
        proposal_content_id: BlockHash(STATE_DIFF_COMMITMENT.0.0),
    });
    content_sender.send(prop_part).await.unwrap();
    let fin_receiver = context
        .validate_proposal(BlockNumber(0), 0, ValidatorId::default(), TIMEOUT, content_receiver)
        .await;
    content_sender.close_channel();
    assert_eq!(fin_receiver.await.unwrap().0.0, STATE_DIFF_COMMITMENT.0.0);

    // Re-proposal: Just asserts this is a known valid proposal.
    context
        .repropose(
            BlockHash(STATE_DIFF_COMMITMENT.0.0),
            ProposalInit { height: BlockNumber(0), ..Default::default() },
        )
        .await;
}

#[tokio::test]
async fn proposals_from_different_rounds() {
    let mut batcher = MockBatcherClient::new();
    let proposal_id: Arc<OnceLock<ProposalId>> = Arc::new(OnceLock::new());
    let proposal_id_clone = Arc::clone(&proposal_id);
    batcher.expect_validate_block().returning(move |input: ValidateBlockInput| {
        proposal_id_clone.set(input.proposal_id).unwrap();
        Ok(())
    });
    batcher
        .expect_start_height()
        .withf(|input| input.height == BlockNumber(0))
        .return_once(|_| Ok(()));
    let proposal_id_clone = Arc::clone(&proposal_id);
    batcher.expect_send_proposal_content().times(1).returning(
        move |input: SendProposalContentInput| {
            assert_eq!(input.proposal_id, *proposal_id_clone.get().unwrap());
            let SendProposalContent::Txs(txs) = input.content else {
                panic!("Expected SendProposalContent::Txs, got {:?}", input.content);
            };
            assert_eq!(txs, *TX_BATCH);
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
    let (mut context, _network) = setup(batcher);
    // Initialize the context for a specific height, starting with round 0.
    context.set_height_and_round(BlockNumber(0), 0).await;
    context.set_height_and_round(BlockNumber(0), 1).await;

    // Proposal parts sent in the proposals.
    let prop_part_txs = ProposalPart::Transactions(TransactionBatch {
        transactions: TX_BATCH.clone().into_iter().map(Transaction::from).collect(),
        tx_hashes: vec![TX_BATCH[0].tx_hash()],
    });
    let prop_part_fin = ProposalPart::Fin(ProposalFin {
        proposal_content_id: BlockHash(STATE_DIFF_COMMITMENT.0.0),
    });

    // The proposal from the past round is ignored.
    let (mut content_sender, content_receiver) = mpsc::channel(CHANNEL_SIZE);
    content_sender.send(prop_part_txs.clone()).await.unwrap();

    let fin_receiver_past_round = context
        .validate_proposal(BlockNumber(0), 0, ValidatorId::default(), TIMEOUT, content_receiver)
        .await;
    // No fin was sent, channel remains open.
    assert!(fin_receiver_past_round.await.is_err());

    // The proposal from the current round should be validated.
    let (mut content_sender, content_receiver) = mpsc::channel(CHANNEL_SIZE);
    content_sender.send(prop_part_txs.clone()).await.unwrap();
    content_sender.send(prop_part_fin.clone()).await.unwrap();
    let fin_receiver_curr_round = context
        .validate_proposal(BlockNumber(0), 1, ValidatorId::default(), TIMEOUT, content_receiver)
        .await;
    assert_eq!(fin_receiver_curr_round.await.unwrap().0.0, STATE_DIFF_COMMITMENT.0.0);

    // The proposal from the future round should not be processed.
    let (mut content_sender, content_receiver) = mpsc::channel(CHANNEL_SIZE);
    content_sender.send(prop_part_txs.clone()).await.unwrap();
    content_sender.send(prop_part_fin.clone()).await.unwrap();
    let fin_receiver_future_round = context
        .validate_proposal(BlockNumber(0), 2, ValidatorId::default(), TIMEOUT, content_receiver)
        .await;
    content_sender.close_channel();
    // Even with sending fin and closing the channel.
    assert!(fin_receiver_future_round.now_or_never().is_none());
}

#[tokio::test]
async fn interrupt_active_proposal() {
    let mut batcher = MockBatcherClient::new();
    batcher
        .expect_start_height()
        .withf(|input| input.height == BlockNumber(0))
        .return_once(|_| Ok(()));
    batcher
        .expect_validate_block()
        .times(1)
        .withf(|input| input.proposal_id == ProposalId(0))
        .returning(|_| Ok(()));
    batcher
        .expect_validate_block()
        .times(1)
        .withf(|input| input.proposal_id == ProposalId(1))
        .returning(|_| Ok(()));
    batcher
        .expect_send_proposal_content()
        .withf(|input| {
            input.proposal_id == ProposalId(1)
                && input.content == SendProposalContent::Txs(TX_BATCH.clone())
        })
        .times(1)
        .returning(move |_| {
            Ok(SendProposalContentResponse { response: ProposalStatus::Processing })
        });
    batcher
        .expect_send_proposal_content()
        .withf(|input| {
            input.proposal_id == ProposalId(1)
                && matches!(input.content, SendProposalContent::Finish)
        })
        .times(1)
        .returning(move |_| {
            Ok(SendProposalContentResponse {
                response: ProposalStatus::Finished(ProposalCommitment {
                    state_diff_commitment: STATE_DIFF_COMMITMENT,
                }),
            })
        });
    let (mut context, _network) = setup(batcher);
    // Initialize the context for a specific height, starting with round 0.
    context.set_height_and_round(BlockNumber(0), 0).await;

    // Keep the sender open, as closing it or sending Fin would cause the validate to complete
    // without needing interrupt.
    let (mut _content_sender_0, content_receiver) = mpsc::channel(CHANNEL_SIZE);
    let fin_receiver_0 = context
        .validate_proposal(BlockNumber(0), 0, ValidatorId::default(), TIMEOUT, content_receiver)
        .await;

    let (mut content_sender_1, content_receiver) = mpsc::channel(CHANNEL_SIZE);
    content_sender_1
        .send(ProposalPart::Transactions(TransactionBatch {
            transactions: TX_BATCH.clone().into_iter().map(Transaction::from).collect(),
            tx_hashes: vec![TX_BATCH[0].tx_hash()],
        }))
        .await
        .unwrap();
    content_sender_1
        .send(ProposalPart::Fin(ProposalFin {
            proposal_content_id: BlockHash(STATE_DIFF_COMMITMENT.0.0),
        }))
        .await
        .unwrap();
    let fin_receiver_1 = context
        .validate_proposal(BlockNumber(0), 1, ValidatorId::default(), TIMEOUT, content_receiver)
        .await;
    // Move the context to the next round.
    context.set_height_and_round(BlockNumber(0), 1).await;

    // Interrupt active proposal.
    assert!(fin_receiver_0.await.is_err());
    assert_eq!(fin_receiver_1.await.unwrap().0.0, STATE_DIFF_COMMITMENT.0.0);
}
