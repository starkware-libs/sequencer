use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollo_batcher_types::batcher_types::{
    ProposalCommitment,
    ProposalId,
    ProposalStatus,
    SendProposalContent,
    SendProposalContentInput,
    SendProposalContentResponse,
};
use apollo_batcher_types::communication::BatcherClientError;
use apollo_infra::component_client::ClientError;
use apollo_protobuf::consensus::{ ProposalFin, TransactionBatch, ProposalPart};
use assert_matches::assert_matches;
use starknet_api::hash::PoseidonHash;
use starknet_types_core::felt::Felt;

use futures::channel::{mpsc, oneshot};
use futures::SinkExt;
use num_rational::Ratio;
use starknet_api::block::{BlockHash, BlockNumber, GasPrice};
use starknet_api::core::{StateDiffCommitment};
use starknet_api::data_availability::L1DataAvailabilityMode;
use tokio_util::sync::CancellationToken;

use crate::config::ContextConfig;
use crate::orchestrator_versioned_constants::VersionedConstants;
use crate::sequencer_consensus_context::BuiltProposals;
use crate::test_utils::{
    block_info,
    create_test_and_network_deps,
    TestDeps,
    CHANNEL_SIZE,
    TIMEOUT,
    TX_BATCH,
};
use crate::utils::GasPriceParams;
use crate::validate_proposal::{
    validate_proposal,
    BlockInfoValidation,
    ProposalValidateArguments,
    ValidateProposalError,
};

struct TestProposalValidateArguments {
    pub deps: TestDeps,
    pub block_info_validation: BlockInfoValidation,
    pub proposal_id: ProposalId,
    pub timeout: Duration,
    pub batcher_timeout_margin: Duration,
    pub valid_proposals: Arc<Mutex<BuiltProposals>>,
    pub content_receiver: mpsc::Receiver<ProposalPart>,
    pub fin_sender: oneshot::Sender<BlockHash>,
    pub gas_price_params: GasPriceParams,
    pub cancel_token: CancellationToken,
}

impl From<TestProposalValidateArguments> for ProposalValidateArguments {
    fn from(args: TestProposalValidateArguments) -> Self {
        ProposalValidateArguments {
            deps: args.deps.into(),
            block_info_validation: args.block_info_validation,
            proposal_id: args.proposal_id,
            timeout: args.timeout,
            batcher_timeout_margin: args.batcher_timeout_margin,
            valid_proposals: args.valid_proposals,
            content_receiver: args.content_receiver,
            fin_sender: args.fin_sender,
            gas_price_params: args.gas_price_params,
            cancel_token: args.cancel_token,
        }
    }
}

fn create_proposal_validate_arguments()
-> (TestProposalValidateArguments, mpsc::Sender<ProposalPart>, oneshot::Receiver<BlockHash>) {
    let (mut deps, _) = create_test_and_network_deps();
    deps.setup_default_expectations();
    let block_info_validation = BlockInfoValidation {
        height: BlockNumber(0),
        block_timestamp_window_seconds: 60,
        previous_block_info: None,
        l1_da_mode: L1DataAvailabilityMode::Blob,
        l2_gas_price_fri: VersionedConstants::latest_constants().min_gas_price,
    };
    let proposal_id = ProposalId(1);
    let timeout = TIMEOUT;
    let batcher_timeout_margin = TIMEOUT;
    let valid_proposals = Arc::new(Mutex::new(BuiltProposals::new()));
    let (content_sender, content_receiver) = mpsc::channel(CHANNEL_SIZE);
    let (fin_sender, fin_receiver) = oneshot::channel();
    let context_config = ContextConfig::default();
    let gas_price_params = GasPriceParams {
        min_l1_gas_price_wei: GasPrice(context_config.min_l1_gas_price_wei),
        max_l1_gas_price_wei: GasPrice(context_config.max_l1_gas_price_wei),
        min_l1_data_gas_price_wei: GasPrice(context_config.min_l1_data_gas_price_wei),
        max_l1_data_gas_price_wei: GasPrice(context_config.max_l1_data_gas_price_wei),
        l1_data_gas_price_multiplier: Ratio::new(
            context_config.l1_data_gas_price_multiplier_ppt,
            1000,
        ),
        l1_gas_tip_wei: GasPrice(context_config.l1_gas_tip_wei),
    };
    let cancel_token = CancellationToken::new();

    (
        TestProposalValidateArguments {
            deps,
            block_info_validation,
            proposal_id,
            timeout,
            batcher_timeout_margin,
            valid_proposals,
            content_receiver,
            fin_sender,
            gas_price_params,
            cancel_token,
        },
        content_sender,
        fin_receiver,
    )
}

#[tokio::test]
async fn interrupt_proposal() {
    let (proposal_args, _content_sender, _fin_receiver) = create_proposal_validate_arguments();
    // Interrupt the proposal.
    proposal_args.cancel_token.cancel();

    let res = validate_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(ValidateProposalError::ProposalInterrupted)));
}

#[tokio::test]
async fn validation_timeout() {
    let (mut proposal_args, _content_sender, _fin_receiver) = create_proposal_validate_arguments();
    // Set a very short timeout to trigger a timeout error.
    proposal_args.timeout = Duration::from_micros(1);

    let res = validate_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(ValidateProposalError::ValidationTimeout)));
}

#[tokio::test]
async fn invalid_proposal_part() {
    let (proposal_args, mut content_sender, _fin_receiver) = create_proposal_validate_arguments();

    content_sender
        .send(ProposalPart::ExecutedTransactionCount(0))
        .await
        .expect("Failed to send proposal part");

    let res = validate_proposal(proposal_args.into()).await;
    assert_matches!(res, Err(ValidateProposalError::InvalidProposalPart(err)) 
        if err.contains("Expected BlockInfo or Fin as second proposal part"));
}

#[tokio::test]
async fn invalid_block_info() {
    let (proposal_args, mut content_sender, _fin_receiver) = create_proposal_validate_arguments();

    let mut block_info = block_info(BlockNumber(0));
    block_info.l2_gas_price_fri =
        GasPrice(proposal_args.block_info_validation.l2_gas_price_fri.0 + 1);
    content_sender
        .send(ProposalPart::BlockInfo(block_info))
        .await
        .expect("Failed to send proposal part");

    let res = validate_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(ValidateProposalError::InvalidBlockInfo(_))));
}

#[tokio::test]
async fn validate_block_fail() {
    let (mut proposal_args, mut content_sender, _fin_receiver) =
        create_proposal_validate_arguments();

    let block_info = block_info(BlockNumber(0));
    content_sender
        .send(ProposalPart::BlockInfo(block_info))
        .await
        .expect("Failed to send proposal part");

    proposal_args.deps.batcher.expect_validate_block().returning(|_| {
        Err(BatcherClientError::ClientError(ClientError::CommunicationFailure("".to_string())))
    });

    let res = validate_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(ValidateProposalError::Batcher(_))));
}

#[tokio::test]
async fn send_executed_transaction_count_more_than_once() {
    let (mut proposal_args, mut content_sender, _fin_receiver) =
        create_proposal_validate_arguments();

    let block_info = block_info(BlockNumber(0));
    content_sender
        .send(ProposalPart::BlockInfo(block_info))
        .await
        .expect("Failed to send proposal part");

    proposal_args.deps.batcher.expect_validate_block().returning(|_| Ok(()));
    // Send executed transaction count more than once.
    content_sender
        .send(ProposalPart::ExecutedTransactionCount(0))
        .await
        .expect("Failed to send proposal part");
    content_sender
        .send(ProposalPart::ExecutedTransactionCount(0))
        .await
        .expect("Failed to send proposal part");
    // Batcher aborts the proposal.
    proposal_args.deps.batcher.expect_send_proposal_content().returning(
        move |input: SendProposalContentInput| {
            assert_eq!(input.proposal_id, proposal_args.proposal_id);
            assert_eq!(input.content, SendProposalContent::Abort);
            Ok(SendProposalContentResponse { response: ProposalStatus::Aborted })
        },
    );
    let res = validate_proposal(proposal_args.into()).await;
    assert_matches!(res, Err(ValidateProposalError::ProposalPartFailed(err)) 
        if err.contains("Received executed transaction count more than once"));
}

#[tokio::test]
async fn receive_fin_without_executed_transaction_count() {
    let (mut proposal_args, mut content_sender, _fin_receiver) =
        create_proposal_validate_arguments();

    let block_info = block_info(BlockNumber(0));
    content_sender
        .send(ProposalPart::BlockInfo(block_info))
        .await
        .expect("Failed to send proposal part");

    proposal_args.deps.batcher.expect_validate_block().returning(|_| Ok(()));
    // Send Fin part without sending executed transaction count.
    content_sender
        .send(ProposalPart::Fin(ProposalFin { proposal_commitment: BlockHash::default() }))
        .await
        .expect("Failed to send proposal part");
    // Batcher aborts the proposal.
    proposal_args.deps.batcher.expect_send_proposal_content().returning(
        move |input: SendProposalContentInput| {
            assert_eq!(input.proposal_id, proposal_args.proposal_id);
            assert_eq!(input.content, SendProposalContent::Abort);
            Ok(SendProposalContentResponse { response: ProposalStatus::Aborted })
        },
    );
    let res = validate_proposal(proposal_args.into()).await;
    assert_matches!(res, Err(ValidateProposalError::ProposalPartFailed(err)) 
        if err.contains("Received Fin without executed transaction count"));
}

#[tokio::test]
async fn receive_txs_after_executed_transaction_count() {
    let (mut proposal_args, mut content_sender, _fin_receiver) =
        create_proposal_validate_arguments();

    let block_info = block_info(BlockNumber(0));
    content_sender
        .send(ProposalPart::BlockInfo(block_info))
        .await
        .expect("Failed to send proposal part");

    proposal_args.deps.batcher.expect_validate_block().returning(|_| Ok(()));
    content_sender
        .send(ProposalPart::ExecutedTransactionCount(0))
        .await
        .expect("Failed to send proposal part");
    // Send transactions after executed transaction count.
    content_sender
        .send(ProposalPart::Transactions(TransactionBatch { transactions: TX_BATCH.clone() }))
        .await
        .expect("Failed to send proposal part");
    // Batcher aborts the proposal.
    proposal_args.deps.batcher.expect_send_proposal_content().returning(
        move |input: SendProposalContentInput| {
            assert_eq!(input.proposal_id, proposal_args.proposal_id);
            assert_eq!(input.content, SendProposalContent::Abort);
            Ok(SendProposalContentResponse { response: ProposalStatus::Aborted })
        },
    );
    let res = validate_proposal(proposal_args.into()).await;
    assert_matches!(res, Err(ValidateProposalError::ProposalPartFailed(err)) 
        if err.contains("Received transactions after executed transaction count"));
}

#[tokio::test]
async fn proposal_fin_mismatch() {
    let (mut proposal_args, mut content_sender, _fin_receiver) =
        create_proposal_validate_arguments();

    let block_info = block_info(BlockNumber(0));
    content_sender
        .send(ProposalPart::BlockInfo(block_info))
        .await
        .expect("Failed to send proposal part");

    proposal_args.deps.batcher.expect_validate_block().returning(|_| Ok(()));
    content_sender
        .send(ProposalPart::ExecutedTransactionCount(0))
        .await
        .expect("Failed to send proposal part");
    // Send Fin part.
    let received_fin = BlockHash::default();
    content_sender
        .send(ProposalPart::Fin(ProposalFin { proposal_commitment: received_fin }))
        .await
        .expect("Failed to send proposal part");
    let built_block = StateDiffCommitment(PoseidonHash(Felt::ONE));
    // Batcher returns a different block hash than the one received in Fin.
    proposal_args.deps.batcher.expect_send_proposal_content().returning(
        move |input: SendProposalContentInput| {
            assert_eq!(input.proposal_id, proposal_args.proposal_id);
            assert_eq!(input.content, SendProposalContent::Finish(0));
            Ok(SendProposalContentResponse {
                response: ProposalStatus::Finished(ProposalCommitment {
                    state_diff_commitment: built_block,
                }),
            })
        },
    );
    let res = validate_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(ValidateProposalError::ProposalFinMismatch)));
}


#[tokio::test]
async fn batcher_returns_invalid_proposal() {
    let (mut proposal_args, mut content_sender, _fin_receiver) =
        create_proposal_validate_arguments();

    let block_info = block_info(BlockNumber(0));
    content_sender
        .send(ProposalPart::BlockInfo(block_info))
        .await
        .expect("Failed to send proposal part");

    proposal_args.deps.batcher.expect_validate_block().returning(|_| Ok(()));
    content_sender
        .send(ProposalPart::ExecutedTransactionCount(0))
        .await
        .expect("Failed to send proposal part");
    content_sender
        .send(ProposalPart::Fin(ProposalFin { proposal_commitment: BlockHash::default() }))
        .await
        .expect("Failed to send proposal part");
    // Batcher returns an invalid proposal status.
    proposal_args.deps.batcher.expect_send_proposal_content().returning(
        move |input: SendProposalContentInput| {
            assert_eq!(input.proposal_id, proposal_args.proposal_id);
            assert_eq!(input.content, SendProposalContent::Finish(0));
            Ok(SendProposalContentResponse {
                response: ProposalStatus::InvalidProposal
            })
        },
    );
    let res = validate_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(ValidateProposalError::InvalidProposalPart(_))));
}
