use std::time::Duration;

use apollo_batcher_types::batcher_types::{
    FinishedProposalInfo,
    FinishedProposalInfoWithoutParent,
    GetProposalContent,
    GetProposalContentResponse,
    ProposalCommitment,
};
use apollo_batcher_types::communication::BatcherClientError;
use apollo_infra::component_client::ClientError;
use apollo_transaction_converter::{MockTransactionConverterTrait, TransactionConverterError};
use assert_matches::assert_matches;
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use starknet_api::block::{BlockHash, BlockNumber, GasPrice};
use starknet_api::block_hash::block_hash_calculator::BlockHeaderCommitments;
use starknet_api::core::ClassHash;
use starknet_api::execution_resources::GasAmount;
use tokio_util::task::AbortOnDropHandle;

use crate::build_proposal::{build_proposal, BuildProposalError};
use crate::cende::MockCendeContext;
use crate::dynamic_gas_price::proposal_commitment_from;
use crate::test_utils::{create_proposal_build_arguments, INTERNAL_TX_BATCH, PARTIAL_BLOCK_HASH};
use crate::utils::RetrospectiveStateCommitmentInfosError;

#[tokio::test]
async fn build_proposal_succeed() {
    let (mut proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    // Setup batcher.
    proposal_args.deps.batcher.expect_propose_block().returning(|_| Ok(()));
    proposal_args.deps.batcher.expect_get_proposal_content().returning(|_| {
        Ok(GetProposalContentResponse {
            content: GetProposalContent::Finished(FinishedProposalInfo {
                artifact: FinishedProposalInfoWithoutParent {
                    proposal_commitment: ProposalCommitment {
                        partial_block_hash: PARTIAL_BLOCK_HASH,
                    },
                    final_n_executed_txs: 0,
                    block_header_commitments: BlockHeaderCommitments::default(),
                    l2_gas_used: GasAmount::default(),
                },
                parent_proposal_commitment: None,
            }),
        })
    });
    // Make sure cende returns on time.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let res = build_proposal(proposal_args.into()).await.unwrap();
    assert_eq!(res, proposal_commitment_from(PARTIAL_BLOCK_HASH, Some(GasPrice::default())));
}

#[tokio::test]
async fn propose_block_fail() {
    let (mut proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    // Setup batcher to return an error on propose_block.
    proposal_args.deps.batcher.expect_propose_block().returning(|_| {
        Err(BatcherClientError::ClientError(ClientError::CommunicationFailure("".to_string())))
    });

    let res = build_proposal(proposal_args.into()).await;
    assert_matches!(
        res,
        Err(BuildProposalError::Batcher(msg, _)) if msg.contains("Failed to initiate build proposal")
    );
}

#[tokio::test]
async fn get_proposal_content_fail() {
    let (mut proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    // Setup batcher to return an error on get_proposal_content.
    proposal_args.deps.batcher.expect_propose_block().returning(|_| Ok(()));
    proposal_args.deps.batcher.expect_get_proposal_content().returning(|_| {
        Err(BatcherClientError::ClientError(ClientError::CommunicationFailure("".to_string())))
    });

    let res = build_proposal(proposal_args.into()).await;
    assert_matches!(
        res,
        Err(BuildProposalError::Batcher(msg, _)) if msg.contains("Failed to get proposal content")
    );
}

#[tokio::test]
async fn interrupt_proposal() {
    let (mut proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    // Setup batcher to return Ok on propose_block.
    proposal_args.deps.batcher.expect_propose_block().returning(|_| Ok(()));
    // Interrupt the proposal.
    proposal_args.cancel_token.cancel();

    let res = build_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(BuildProposalError::Interrupted)));
}

#[tokio::test]
async fn convert_internal_consensus_tx_to_consensus_tx_fail() {
    let (mut proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    // Setup batcher to return Ok on propose_block and TX from get_proposal_content.
    proposal_args.deps.batcher.expect_propose_block().returning(|_| Ok(()));
    proposal_args.deps.batcher.expect_get_proposal_content().times(1).returning(|_| {
        Ok(GetProposalContentResponse {
            content: GetProposalContent::Txs(INTERNAL_TX_BATCH.clone()),
        })
    });
    // Overwrite the transaction converter to return an error, since by default it returns Ok.
    let mut transaction_converter = MockTransactionConverterTrait::new();
    transaction_converter.expect_convert_internal_consensus_tx_to_consensus_tx().returning(|_| {
        Err(TransactionConverterError::ClassNotFound { class_hash: ClassHash::default() })
    });
    proposal_args.deps.transaction_converter = transaction_converter;

    let res = build_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(BuildProposalError::TransactionConverterError(_))));
}

#[tokio::test]
async fn cende_fail() {
    let (mut proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    // Setup batcher to return Ok on propose_block and Finished from get_proposal_content.
    proposal_args.deps.batcher.expect_propose_block().returning(|_| Ok(()));
    proposal_args.deps.batcher.expect_get_proposal_content().times(1).returning(|_| {
        Ok(GetProposalContentResponse {
            content: GetProposalContent::Finished(FinishedProposalInfo {
                artifact: FinishedProposalInfoWithoutParent {
                    proposal_commitment: ProposalCommitment {
                        partial_block_hash: PARTIAL_BLOCK_HASH,
                    },
                    final_n_executed_txs: 0,
                    block_header_commitments: BlockHeaderCommitments::default(),
                    l2_gas_used: GasAmount::default(),
                },
                parent_proposal_commitment: None,
            }),
        })
    });
    // Setup cende to return false, indicating a failure.
    proposal_args.cende_write_success = AbortOnDropHandle::new(tokio::spawn(async { false }));

    let res = build_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(BuildProposalError::CendeWriteError(_))));
}

/// The blob of height H must carry the state commitment infos (witnesses) of the next height's
/// retrospective block, H + 1 - STORED_BLOCK_HASH_BUFFER. When neither the batcher nor the cende
/// recorder has stored them, the round fails before the block is even proposed.
#[tokio::test]
async fn missing_retrospective_state_commitment_infos_fail() {
    let (mut proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    proposal_args.build_param.height = BlockNumber(STORED_BLOCK_HASH_BUFFER);
    let next_height_retro_block_number =
        BlockNumber(proposal_args.build_param.height.0 + 1 - STORED_BLOCK_HASH_BUFFER);

    // Setup the retrospective block hash check to pass.
    proposal_args.deps.batcher.expect_get_block_hash().returning(|_| Ok(BlockHash::default()));
    proposal_args
        .deps
        .state_sync_client
        .expect_get_block_hash()
        .returning(|_| Ok(BlockHash::default()));

    // The batcher doesn't have the retrospective block's state commitment infos, and the cende
    // recorder's height offset shows it hasn't stored them either.
    proposal_args
        .deps
        .batcher
        .expect_has_state_commitment_infos()
        .withf(move |block_number| *block_number == next_height_retro_block_number)
        .returning(|_| Ok(false));
    let mut cende_ambassador = MockCendeContext::new();
    cende_ambassador
        .expect_commitment_infos_height_offset()
        .returning(move || Ok(Some(next_height_retro_block_number)));
    proposal_args.deps.cende_ambassador = cende_ambassador;

    // No `propose_block` expectation is set: reaching the batcher would panic, proving the build
    // fails before proposing.
    let res = build_proposal(proposal_args.into()).await;
    assert_matches!(
        res,
        Err(BuildProposalError::RetrospectiveStateCommitmentInfosError(
            RetrospectiveStateCommitmentInfosError::NotStored { retrospective_block_number, .. }
        )) if retrospective_block_number == next_height_retro_block_number
    );
}
