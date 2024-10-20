use std::sync::{Arc, OnceLock};
use std::time::Duration;
use std::vec;

use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use lazy_static::lazy_static;
use papyrus_consensus::types::{ConsensusContext, ProposalInit};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::StateDiffCommitment;
use starknet_api::executable_transaction::Transaction;
use starknet_api::hash::PoseidonHash;
use starknet_api::test_utils::invoke::{executable_invoke_tx, InvokeTxArgs};
use starknet_api::transaction::TransactionHash;
use starknet_batcher_types::batcher_types::{
    BuildProposalInput,
    GetProposalContent,
    GetProposalContentResponse,
    ProposalCommitment,
    ProposalId,
    ProposalStatus,
    SendProposalContent,
    SendProposalContentInput,
    SendProposalContentResponse,
    StartHeightInput,
    ValidateProposalInput,
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
    let mut context = SequencerConsensusContext::new(Arc::new(batcher), NUM_VALIDATORS);
    let (mut content_receiver, fin_receiver) =
        context.build_proposal(BlockNumber(0), TIMEOUT).await;
    assert_eq!(content_receiver.next().await, Some(TX_BATCH.clone()));
    assert!(content_receiver.next().await.is_none());
    assert_eq!(fin_receiver.await.unwrap().0, STATE_DIFF_COMMITMENT.0.0);
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
    let mut context = SequencerConsensusContext::new(Arc::new(batcher), NUM_VALIDATORS);
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
    batcher.expect_validate_proposal().returning(move |_| Ok(()));
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

    let mut context = SequencerConsensusContext::new(Arc::new(batcher), NUM_VALIDATORS);

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
