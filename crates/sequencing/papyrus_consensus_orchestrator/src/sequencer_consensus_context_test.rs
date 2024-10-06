use std::sync::{Arc, OnceLock};
use std::time::Duration;
use std::vec;

use futures::StreamExt;
use lazy_static::lazy_static;
use papyrus_consensus::types::ConsensusContext;
use starknet_api::block::BlockNumber;
use starknet_api::core::TransactionCommitment;
use starknet_api::executable_transaction::Transaction;
use starknet_api::test_utils::invoke::{executable_invoke_tx, InvokeTxArgs};
use starknet_api::transaction::TransactionHash;
use starknet_batcher_types::batcher_types::{
    BuildProposalInput,
    GetProposalContent,
    GetProposalContentResponse,
    ProposalCommitment,
};
use starknet_batcher_types::communication::MockBatcherClient;
use starknet_types_core::felt::Felt;

use crate::sequencer_consensus_context::SequencerConsensusContext;

const TIMEOUT: Duration = Duration::from_millis(100);
const TX_COMMITMENT: TransactionCommitment = TransactionCommitment(Felt::ZERO);

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
                tx_commitment: TX_COMMITMENT,
                ..Default::default()
            }),
        })
    });
    let mut context = SequencerConsensusContext::new(Arc::new(batcher));
    let (mut content_receiver, fin_receiver) =
        context.build_proposal(BlockNumber(0), TIMEOUT).await;
    assert_eq!(content_receiver.next().await, Some(TX_BATCH.clone()));
    assert!(content_receiver.next().await.is_none());
    assert_eq!(fin_receiver.await.unwrap().0, TX_COMMITMENT.0);
}
