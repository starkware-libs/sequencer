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
use apollo_consensus_orchestrator_config::ContextConfig;
use apollo_infra::component_client::ClientError;
use apollo_protobuf::consensus::{ProposalFin, ProposalPart, TransactionBatch};
use assert_matches::assert_matches;
use futures::channel::mpsc;
use futures::SinkExt;
use num_rational::Ratio;
use rstest::rstest;
use starknet_api::block::{BlockHash, BlockNumber, GasPrice};
use starknet_api::core::StateDiffCommitment;
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::hash::PoseidonHash;
use starknet_types_core::felt::Felt;
use tokio_util::sync::CancellationToken;

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
    within_margin,
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
            gas_price_params: args.gas_price_params,
            cancel_token: args.cancel_token,
        }
    }
}

fn create_proposal_validate_arguments()
-> (TestProposalValidateArguments, mpsc::Sender<ProposalPart>) {
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
            gas_price_params,
            cancel_token,
        },
        content_sender,
    )
}

#[tokio::test]
async fn validate_empty_proposal() {
    let (proposal_args, mut content_sender) = create_proposal_validate_arguments();
    // Send an empty proposal.
    content_sender
        .send(ProposalPart::Fin(ProposalFin { proposal_commitment: BlockHash::default() }))
        .await
        .unwrap();

    let res = validate_proposal(proposal_args.into()).await;
    assert_matches!(res, Ok(val) if val == BlockHash::default());
}

#[tokio::test]
async fn validate_proposal_success() {
    let (mut proposal_args, mut content_sender) = create_proposal_validate_arguments();
    let n_executed = 1;
    // Setup deps to validate the block.
    proposal_args.deps.setup_deps_for_validate(BlockNumber(0), n_executed);
    // Send a valid block info.
    let block_info = block_info(BlockNumber(0));
    content_sender.send(ProposalPart::BlockInfo(block_info)).await.unwrap();
    // Send transactions, then executed transaction count, and finally Fin part.
    content_sender
        .send(ProposalPart::Transactions(TransactionBatch { transactions: TX_BATCH.clone() }))
        .await
        .unwrap();
    content_sender
        .send(ProposalPart::ExecutedTransactionCount(n_executed.try_into().unwrap()))
        .await
        .unwrap();
    content_sender
        .send(ProposalPart::Fin(ProposalFin { proposal_commitment: BlockHash::default() }))
        .await
        .unwrap();

    let res = validate_proposal(proposal_args.into()).await;
    assert_matches!(res, Ok(val) if val == BlockHash::default());
}

#[tokio::test]
async fn interrupt_proposal() {
    let (proposal_args, _content_sender) = create_proposal_validate_arguments();
    // Interrupt the proposal.
    proposal_args.cancel_token.cancel();

    let res = validate_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(ValidateProposalError::ProposalInterrupted(_))));
}

#[tokio::test]
async fn validation_timeout() {
    let (mut proposal_args, _content_sender) = create_proposal_validate_arguments();
    // Set a very short timeout to trigger a timeout error.
    proposal_args.timeout = Duration::from_micros(1);

    let res = validate_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(ValidateProposalError::ValidationTimeout(_))));
}

#[tokio::test]
async fn invalid_second_proposal_part() {
    let (proposal_args, mut content_sender) = create_proposal_validate_arguments();
    // Send an invalid proposal part (not BlockInfo or Fin).
    content_sender.send(ProposalPart::ExecutedTransactionCount(0)).await.unwrap();

    let res = validate_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(ValidateProposalError::InvalidSecondProposalPart(_))));
}

#[tokio::test]
async fn invalid_block_info() {
    let (proposal_args, mut content_sender) = create_proposal_validate_arguments();

    let mut block_info = block_info(BlockNumber(0));
    block_info.l2_gas_price_fri =
        GasPrice(proposal_args.block_info_validation.l2_gas_price_fri.0 + 1);
    content_sender.send(ProposalPart::BlockInfo(block_info)).await.unwrap();

    let res = validate_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(ValidateProposalError::InvalidBlockInfo(_, _, _))));
}

#[tokio::test]
async fn validate_block_fail() {
    let (mut proposal_args, mut content_sender) = create_proposal_validate_arguments();
    // Setup batcher to return an error when validating the block.
    proposal_args.deps.batcher.expect_validate_block().returning(|_| {
        Err(BatcherClientError::ClientError(ClientError::CommunicationFailure("".to_string())))
    });
    // Send a valid block info.
    let block_info = block_info(BlockNumber(0));
    content_sender.send(ProposalPart::BlockInfo(block_info)).await.unwrap();

    let res = validate_proposal(proposal_args.into()).await;
    assert_matches!(res, Err(ValidateProposalError::Batcher(msg,_ ))
        if msg.contains("Failed to initiate validate proposal"));
}

#[tokio::test]
async fn send_executed_transaction_count_more_than_once() {
    let (mut proposal_args, mut content_sender) = create_proposal_validate_arguments();
    // Setup batcher to validate the block.
    proposal_args.deps.batcher.expect_validate_block().returning(|_| Ok(()));
    // Batcher aborts the proposal.
    proposal_args
        .deps
        .batcher
        .expect_send_proposal_content()
        .withf(move |input: &SendProposalContentInput| {
            input.proposal_id == proposal_args.proposal_id
                && input.content == SendProposalContent::Abort
        })
        .returning(|_| Ok(SendProposalContentResponse { response: ProposalStatus::Aborted }));
    // Send a valid block info.
    let block_info = block_info(BlockNumber(0));
    content_sender.send(ProposalPart::BlockInfo(block_info)).await.unwrap();
    // Send executed transaction count more than once.
    content_sender.send(ProposalPart::ExecutedTransactionCount(0)).await.unwrap();
    content_sender.send(ProposalPart::ExecutedTransactionCount(0)).await.unwrap();

    let res = validate_proposal(proposal_args.into()).await;
    assert_matches!(res, Err(ValidateProposalError::ProposalPartFailed(err,_))
        if err.contains("Received executed transaction count more than once"));
}

#[tokio::test]
async fn receive_fin_without_executed_transaction_count() {
    let (mut proposal_args, mut content_sender) = create_proposal_validate_arguments();
    // Setup batcher to validate the block.
    proposal_args.deps.batcher.expect_validate_block().returning(|_| Ok(()));
    // Batcher aborts the proposal.
    proposal_args
        .deps
        .batcher
        .expect_send_proposal_content()
        .withf(move |input: &SendProposalContentInput| {
            input.proposal_id == proposal_args.proposal_id
                && input.content == SendProposalContent::Abort
        })
        .returning(|_| Ok(SendProposalContentResponse { response: ProposalStatus::Aborted }));
    // Send a valid block info.
    let block_info = block_info(BlockNumber(0));
    content_sender.send(ProposalPart::BlockInfo(block_info)).await.unwrap();
    // Send Fin part without sending executed transaction count.
    content_sender
        .send(ProposalPart::Fin(ProposalFin { proposal_commitment: BlockHash::default() }))
        .await
        .unwrap();

    let res = validate_proposal(proposal_args.into()).await;
    assert_matches!(res, Err(ValidateProposalError::ProposalPartFailed(err,_))
        if err.contains("Received Fin without executed transaction count"));
}

#[tokio::test]
async fn receive_txs_after_executed_transaction_count() {
    let (mut proposal_args, mut content_sender) = create_proposal_validate_arguments();
    // Setup batcher to validate the block.
    proposal_args.deps.batcher.expect_validate_block().returning(|_| Ok(()));
    // Batcher aborts the proposal.
    proposal_args
        .deps
        .batcher
        .expect_send_proposal_content()
        .withf(move |input: &SendProposalContentInput| {
            input.proposal_id == proposal_args.proposal_id
                && input.content == SendProposalContent::Abort
        })
        .returning(|_| Ok(SendProposalContentResponse { response: ProposalStatus::Aborted }));
    // Send a valid block info.
    let block_info = block_info(BlockNumber(0));
    content_sender.send(ProposalPart::BlockInfo(block_info)).await.unwrap();
    content_sender.send(ProposalPart::ExecutedTransactionCount(0)).await.unwrap();
    // Send transactions after executed transaction count.
    content_sender
        .send(ProposalPart::Transactions(TransactionBatch { transactions: TX_BATCH.clone() }))
        .await
        .unwrap();

    let res = validate_proposal(proposal_args.into()).await;
    assert_matches!(res, Err(ValidateProposalError::ProposalPartFailed(err,_))
        if err.contains("Received transactions after executed transaction count"));
}

#[tokio::test]
async fn proposal_fin_mismatch() {
    let (mut proposal_args, mut content_sender) = create_proposal_validate_arguments();
    let n_executed = 0;
    // Setup batcher to validate the block.
    proposal_args.deps.batcher.expect_validate_block().returning(|_| Ok(()));
    // Batcher returns a different block hash than the one received in Fin.
    let built_block = StateDiffCommitment(PoseidonHash(Felt::ONE));
    proposal_args
        .deps
        .batcher
        .expect_send_proposal_content()
        .withf(move |input: &SendProposalContentInput| {
            input.proposal_id == proposal_args.proposal_id
                && input.content == SendProposalContent::Finish(n_executed)
        })
        .returning(move |_| {
            Ok(SendProposalContentResponse {
                response: ProposalStatus::Finished(ProposalCommitment {
                    state_diff_commitment: built_block,
                }),
            })
        });
    // Send a valid block info.
    let block_info = block_info(BlockNumber(0));
    content_sender.send(ProposalPart::BlockInfo(block_info)).await.unwrap();
    content_sender
        .send(ProposalPart::ExecutedTransactionCount(n_executed.try_into().unwrap()))
        .await
        .unwrap();
    // Send Fin part.
    let received_fin = BlockHash::default();
    content_sender
        .send(ProposalPart::Fin(ProposalFin { proposal_commitment: received_fin }))
        .await
        .unwrap();

    let res = validate_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(ValidateProposalError::ProposalFinMismatch)));
}

#[tokio::test]
async fn batcher_returns_invalid_proposal() {
    let (mut proposal_args, mut content_sender) = create_proposal_validate_arguments();
    let n_executed = 0;
    // Setup batcher to validate the block.
    proposal_args.deps.batcher.expect_validate_block().returning(|_| Ok(()));
    // Batcher returns an invalid proposal status.
    proposal_args
        .deps
        .batcher
        .expect_send_proposal_content()
        .withf(move |input: &SendProposalContentInput| {
            input.proposal_id == proposal_args.proposal_id
                && input.content == SendProposalContent::Finish(n_executed)
        })
        .returning(|_| {
            Ok(SendProposalContentResponse {
                response: ProposalStatus::InvalidProposal("test error".to_string()),
            })
        });
    // Send a valid block info.
    let block_info = block_info(BlockNumber(0));
    content_sender.send(ProposalPart::BlockInfo(block_info)).await.unwrap();
    content_sender
        .send(ProposalPart::ExecutedTransactionCount(n_executed.try_into().unwrap()))
        .await
        .unwrap();
    content_sender
        .send(ProposalPart::Fin(ProposalFin { proposal_commitment: BlockHash::default() }))
        .await
        .unwrap();

    let res = validate_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(ValidateProposalError::InvalidProposal(_))));
}

#[rstest]
#[case::big_number_in_margin(1000, 1050, 10, true)]
#[case::big_number_out_of_margin(1000, 1150, 10, false)]
#[case::small_number_in_margin(9, 10, 10, true)]
#[case::small_number_out_of_margin(9, 11, 10, false)]
#[case::identical_numbers(12345, 12345, 1, true)]
fn test_within_margin(
    #[case] a: u128,
    #[case] b: u128,
    #[case] margin: u128,
    #[case] expected: bool,
) {
    assert_eq!(within_margin(GasPrice(a), GasPrice(b), margin), expected);
}
