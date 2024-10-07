use std::sync::{Arc, OnceLock};
use std::time::Duration;
use std::vec;

use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use papyrus_consensus::types::ConsensusContext;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::TransactionCommitment;
use starknet_api::executable_transaction::Transaction;
use starknet_api::test_utils::invoke::{executable_invoke_tx, InvokeTxArgs};
use starknet_api::transaction::TransactionHash;
use starknet_batcher_types::batcher_types::{
    BuildProposalInput,
    GetProposalContent,
    GetProposalContentResponse,
    ProposalCommitment,
    ProposalStatus,
    SendProposalContent,
    SendProposalContentInput,
    SendProposalContentResponse,
    ValidateProposalInput,
};
use starknet_batcher_types::communication::MockBatcherClient;
use starknet_consensus_manager_types::consensus_manager_types::ProposalId;
use starknet_types_core::felt::Felt;

use crate::sequencer_consensus_context::SequencerConsensusContext;

const TIMEOUT: Duration = Duration::from_millis(100);
const CHANNEL_SIZE: usize = 5000;
const NUM_VALIDATORS: u64 = 4;

fn generate_invoke_tx(tx_hash: Felt) -> Transaction {
    Transaction::Invoke(executable_invoke_tx(InvokeTxArgs {
        tx_hash: TransactionHash(tx_hash),
        ..Default::default()
    }))
}

#[tokio::test]
async fn build_proposal() {
    let mut batcher = MockBatcherClient::new();
    let proposal_id = Arc::new(OnceLock::new());
    let proposal_id_clone = Arc::clone(&proposal_id);
    batcher.expect_build_proposal().returning(move |input: BuildProposalInput| {
        proposal_id_clone.set(input.proposal_id).unwrap();
        Ok(())
    });
    let proposal_id_clone = Arc::clone(&proposal_id);
    batcher.expect_get_proposal_content().times(1).returning(move |input| {
        assert_eq!(input.proposal_id, *proposal_id_clone.get().unwrap());
        Ok(GetProposalContentResponse { content: GetProposalContent::Txs(vec![]) })
    });
    let proposal_id_clone = Arc::clone(&proposal_id);
    batcher.expect_get_proposal_content().times(1).returning(move |input| {
        assert_eq!(input.proposal_id, *proposal_id_clone.get().unwrap());
        Ok(GetProposalContentResponse {
            content: GetProposalContent::Finished(ProposalCommitment {
                tx_commitment: TransactionCommitment(Felt::ZERO),
                ..Default::default()
            }),
        })
    });
    let mut context = SequencerConsensusContext::new(Arc::new(batcher), NUM_VALIDATORS);
    let (mut content_receiver, fin_receiver) =
        context.build_proposal(BlockNumber(0), TIMEOUT).await;
    assert_eq!(content_receiver.next().await, Some(vec![]));
    assert!(content_receiver.next().await.is_none());
    assert_eq!(fin_receiver.await.unwrap().0, Felt::ZERO);
}

#[tokio::test]
async fn validate_proposal_success() {
    let mut batcher = MockBatcherClient::new();
    let proposal_id: Arc<OnceLock<ProposalId>> = Arc::new(OnceLock::new());
    let proposal_id_clone = Arc::clone(&proposal_id);
    batcher.expect_validate_proposal().returning(move |input: ValidateProposalInput| {
        proposal_id_clone.set(input.proposal_id).unwrap();
        Ok(())
    });
    let proposal_id_clone = Arc::clone(&proposal_id);
    batcher.expect_send_proposal_content().times(1).returning(
        move |input: SendProposalContentInput| {
            assert_eq!(input.proposal_id, *proposal_id_clone.get().unwrap());
            let SendProposalContent::Txs(txs) = input.content else {
                panic!("Expected SendProposalContent::Txs, got {:?}", input.content);
            };
            assert_eq!(txs, vec![generate_invoke_tx(Felt::TWO)]);
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
                    tx_commitment: TransactionCommitment(Felt::ONE),
                    ..Default::default()
                }),
            })
        },
    );
    let mut context = SequencerConsensusContext::new(Arc::new(batcher), NUM_VALIDATORS);
    let (mut content_sender, content_receiver) = mpsc::channel(CHANNEL_SIZE);
    content_sender.send(vec![generate_invoke_tx(Felt::TWO)]).await.unwrap();
    let fin_receiver = context.validate_proposal(BlockNumber(0), TIMEOUT, content_receiver).await;
    content_sender.close_channel();
    assert_eq!(fin_receiver.await.unwrap().0, Felt::ONE);
}

#[tokio::test]
async fn get_proposal() {
    const CONTENT_ID: Felt = Felt::ONE;
    // Receive a proposal. Then re-retrieve it.
    let mut batcher = MockBatcherClient::new();
    batcher.expect_validate_proposal().returning(move |_| Ok(()));
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
                    tx_commitment: TransactionCommitment(CONTENT_ID),
                    ..Default::default()
                }),
            })
        },
    );

    let mut context = SequencerConsensusContext::new(Arc::new(batcher), NUM_VALIDATORS);

    // Receive a valid proposal.
    let (mut content_sender, content_receiver) = mpsc::channel(CHANNEL_SIZE);
    let txs = vec![generate_invoke_tx(Felt::TWO)];
    content_sender.send(txs.clone()).await.unwrap();
    let fin_receiver = context.validate_proposal(BlockNumber(0), TIMEOUT, content_receiver).await;
    content_sender.close_channel();
    assert_eq!(fin_receiver.await.unwrap().0, CONTENT_ID);

    // Re-proposal.
    let mut reproposal_content = context.get_proposal(BlockNumber(0), BlockHash(CONTENT_ID)).await;
    assert_eq!(reproposal_content.next().await, Some(txs));
    assert!(reproposal_content.next().await.is_none());
}
