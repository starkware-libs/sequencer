use std::sync::Arc;
use std::time::Duration;

use apollo_batcher_types::batcher_types::{
    GetProposalContent,
    GetProposalContentResponse,
    ProposalCommitment,
};
use apollo_batcher_types::communication::{BatcherClientError, MockBatcherClient};
use apollo_class_manager_types::transaction_converter::{
    MockTransactionConverterTrait,
    TransactionConverterError,
};
use apollo_infra::component_client::ClientError;
use apollo_state_sync_types::communication::{MockStateSyncClient, StateSyncClientError};
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ClassHash;
use tokio_util::task::AbortOnDropHandle;

use crate::build_proposal::{build_proposal, BuildProposalError};
use crate::test_utils::{
    create_proposal_build_arguments,
    INTERNAL_TX_BATCH,
    STATE_DIFF_COMMITMENT,
};

#[tokio::test]
async fn build_proposal_succeed() {
    let (mut proposal_args, _proposal_receiver, _fin_receiver) = create_proposal_build_arguments();
    // Setup batcher.
    let mut batcher = MockBatcherClient::new();
    batcher.expect_propose_block().returning(|_| Ok(()));
    batcher.expect_get_proposal_content().returning(|_| {
        Ok(GetProposalContentResponse {
            content: GetProposalContent::Finished {
                id: ProposalCommitment { state_diff_commitment: STATE_DIFF_COMMITMENT },
                final_n_executed_txs: 0,
            },
        })
    });
    proposal_args.deps.batcher = Arc::new(batcher);
    // Make sure cende returns on time.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let res = build_proposal(proposal_args).await.unwrap();
    assert_eq!(res, BlockHash::default());
}

#[tokio::test]
async fn state_sync_client_error() {
    let (mut proposal_args, _proposal_receiver, _fin_receiver) = create_proposal_build_arguments();
    // Make sure state_sync_client being called, by setting height to >= STORED_BLOCK_HASH_BUFFER.
    proposal_args.proposal_init.height = BlockNumber(STORED_BLOCK_HASH_BUFFER);
    // Setup state sync client to return an error.
    let mut state_sync_client = MockStateSyncClient::new();
    state_sync_client.expect_get_block().returning(|_| {
        Err(StateSyncClientError::ClientError(ClientError::CommunicationFailure("".to_string())))
    });
    proposal_args.deps.state_sync_client = Arc::new(state_sync_client);

    let res = build_proposal(proposal_args).await;
    assert!(matches!(res, Err(BuildProposalError::StateSyncClientError(_))));
}

#[tokio::test]
async fn state_sync_not_ready_error() {
    let (mut proposal_args, _proposal_receiver, _fin_receiver) = create_proposal_build_arguments();
    // Make sure state_sync_client being called, by setting height to >= STORED_BLOCK_HASH_BUFFER.
    proposal_args.proposal_init.height = BlockNumber(STORED_BLOCK_HASH_BUFFER);
    // Setup state sync client to return None, indicating that the state sync is not ready.
    let mut state_sync_client = MockStateSyncClient::new();
    state_sync_client.expect_get_block().returning(|_| Ok(None));
    proposal_args.deps.state_sync_client = Arc::new(state_sync_client);

    let res = build_proposal(proposal_args).await;
    assert!(matches!(res, Err(BuildProposalError::StateSyncNotReady(_))));
}

#[tokio::test]
async fn propose_block_fail() {
    let (mut proposal_args, _proposal_receiver, _fin_receiver) = create_proposal_build_arguments();
    // Setup batcher to return an error on propose_block.
    let mut batcher = MockBatcherClient::new();
    batcher.expect_propose_block().returning(|_| {
        Err(BatcherClientError::ClientError(ClientError::CommunicationFailure("".to_string())))
    });
    proposal_args.deps.batcher = Arc::new(batcher);

    let res = build_proposal(proposal_args).await;
    assert!(matches!(res, Err(BuildProposalError::Batcher(_))));
}

#[tokio::test]
async fn get_proposal_content_fail() {
    let (mut proposal_args, _proposal_receiver, _fin_receiver) = create_proposal_build_arguments();
    // Setup batcher to return an error on get_proposal_content.
    let mut batcher = MockBatcherClient::new();
    batcher.expect_propose_block().returning(|_| Ok(()));
    batcher.expect_get_proposal_content().returning(|_| {
        Err(BatcherClientError::ClientError(ClientError::CommunicationFailure("".to_string())))
    });
    proposal_args.deps.batcher = Arc::new(batcher);

    let res = build_proposal(proposal_args).await;
    assert!(matches!(res, Err(BuildProposalError::Batcher(_))));
}

#[tokio::test]
async fn interrupt_proposal() {
    let (mut proposal_args, _proposal_receiver, _fin_receiver) = create_proposal_build_arguments();
    // Setup batcher to return Ok on propose_block.
    let mut batcher = MockBatcherClient::new();
    batcher.expect_propose_block().returning(|_| Ok(()));
    proposal_args.deps.batcher = Arc::new(batcher);
    // Interrupt the proposal.
    proposal_args.cancel_token.cancel();

    let res = build_proposal(proposal_args).await;
    assert!(matches!(res, Err(BuildProposalError::Interrupted)));
}

#[tokio::test]
async fn convert_internal_consensus_tx_to_consensus_tx_fail() {
    let (mut proposal_args, _proposal_receiver, _fin_receiver) = create_proposal_build_arguments();
    // Setup batcher to return Ok on propose_block and TX from get_proposal_content.
    let mut batcher = MockBatcherClient::new();
    batcher.expect_propose_block().returning(|_| Ok(()));
    batcher.expect_get_proposal_content().times(1).returning(|_| {
        Ok(GetProposalContentResponse {
            content: GetProposalContent::Txs(INTERNAL_TX_BATCH.clone()),
        })
    });
    proposal_args.deps.batcher = Arc::new(batcher);
    // Setup transaction converter to return an error.
    let mut transaction_converter = MockTransactionConverterTrait::new();
    transaction_converter.expect_convert_internal_consensus_tx_to_consensus_tx().returning(|_| {
        Err(TransactionConverterError::ClassNotFound { class_hash: ClassHash::default() })
    });
    proposal_args.deps.transaction_converter = Arc::new(transaction_converter);

    let res = build_proposal(proposal_args).await;
    assert!(matches!(res, Err(BuildProposalError::TransactionConverterError(_))));
}

#[tokio::test]
async fn cende_fail() {
    let (mut proposal_args, _proposal_receiver, _fin_receiver) = create_proposal_build_arguments();
    // Setup batcher to return Ok on propose_block and Finished from get_proposal_content.
    let mut batcher = MockBatcherClient::new();
    batcher.expect_propose_block().returning(|_| Ok(()));
    batcher.expect_get_proposal_content().times(1).returning(|_| {
        Ok(GetProposalContentResponse {
            content: GetProposalContent::Finished {
                id: ProposalCommitment { state_diff_commitment: STATE_DIFF_COMMITMENT },
                final_n_executed_txs: 0,
            },
        })
    });
    proposal_args.deps.batcher = Arc::new(batcher);
    // Setup cende to return false, indicating a failure.
    proposal_args.cende_write_success = AbortOnDropHandle::new(tokio::spawn(async { false }));

    let res = build_proposal(proposal_args).await;
    assert!(matches!(res, Err(BuildProposalError::CendeWriteError(_))));
}
