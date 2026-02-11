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
use apollo_consensus_orchestrator_config::config::ContextConfig;
use apollo_infra::component_client::ClientError;
use apollo_protobuf::consensus::{
    ProposalCommitment as ConsensusProposalCommitment,
    ProposalFin,
    ProposalInit,
    SignedProposalPart,
    TransactionBatch,
};
use assert_matches::assert_matches;
use futures::channel::mpsc;
use futures::SinkExt;
use rstest::rstest;
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::core::StateDiffCommitment;
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::hash::PoseidonHash;
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use starknet_types_core::felt::Felt;
use tokio_util::sync::CancellationToken;

use crate::orchestrator_versioned_constants::VersionedConstants;
use crate::sequencer_consensus_context::BuiltProposals;
use crate::test_utils::{
    block_info,
    create_test_and_network_deps,
    SetupDepsArgs,
    TestDeps,
    CHANNEL_SIZE,
    TIMEOUT,
    TX_BATCH,
};
use crate::utils::{make_gas_price_params, GasPriceParams};
use crate::validate_proposal::{
    validate_proposal,
    within_margin,
    BlockInfoValidation,
    ProposalValidateArguments,
    ValidateProposalError,
};

struct TestProposalValidateArguments {
    pub deps: TestDeps,
    pub init: ProposalInit,
    pub block_info_validation: BlockInfoValidation,
    pub proposal_id: ProposalId,
    pub timeout: Duration,
    pub batcher_timeout_margin: Duration,
    pub valid_proposals: Arc<Mutex<BuiltProposals>>,
    pub content_receiver: mpsc::Receiver<SignedProposalPart>,
    pub gas_price_params: GasPriceParams,
    pub cancel_token: CancellationToken,
}

impl From<TestProposalValidateArguments> for ProposalValidateArguments {
    fn from(args: TestProposalValidateArguments) -> Self {
        ProposalValidateArguments {
            deps: args.deps.into(),
            init: args.init,
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
-> (TestProposalValidateArguments, mpsc::Sender<SignedProposalPart>) {
    let (mut deps, _) = create_test_and_network_deps();
    deps.setup_default_expectations();
    let init = block_info(BlockNumber(0), 0);
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
    let gas_price_params = make_gas_price_params(&context_config.dynamic_config);
    let cancel_token = CancellationToken::new();

    (
        TestProposalValidateArguments {
            deps,
            init,
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
    let (mut proposal_args, mut content_sender) = create_proposal_validate_arguments();
    // Empty proposals call validate_block and send Finish (no Txs)
    proposal_args.deps.batcher.expect_validate_block().times(1).returning(|_| Ok(()));
    proposal_args
        .deps
        .batcher
        .expect_start_height()
        .withf(|input| input.height == BlockNumber(0))
        .return_const(Ok(()));
    proposal_args.deps.batcher.expect_send_proposal_content().times(1).returning(|input| {
        assert!(matches!(input.content, SendProposalContent::Finish(_)));
        Ok(SendProposalContentResponse {
            response: ProposalStatus::Finished(ProposalCommitment::default()),
        })
    });

    // Send an empty proposal.
    content_sender
        .send(SignedProposalPart::fin(ProposalFin {
            proposal_commitment: ConsensusProposalCommitment::default(),
            executed_transaction_count: 0,
            commitment_parts: None,
        }))
        .await
        .unwrap();

    let res = validate_proposal(proposal_args.into()).await;
    assert_matches!(res, Ok(val) if val == ConsensusProposalCommitment::default());
}

#[tokio::test]
async fn validate_proposal_success() {
    let (mut proposal_args, mut content_sender) = create_proposal_validate_arguments();
    let n_executed_txs_count = 1;
    // Setup deps to validate the block.
    proposal_args.deps.setup_deps_for_validate(SetupDepsArgs {
        n_executed_txs_count,
        expect_start_height: false,
        ..Default::default()
    });
    // Send transactions and finally Fin part with executed transaction count.
    content_sender
        .send(SignedProposalPart::transactions(TransactionBatch { transactions: TX_BATCH.clone() }))
        .await
        .unwrap();
    content_sender
        .send(SignedProposalPart::fin(ProposalFin {
            proposal_commitment: ConsensusProposalCommitment::default(),
            executed_transaction_count: n_executed_txs_count.try_into().unwrap(),
            commitment_parts: None,
        }))
        .await
        .unwrap();

    let res = validate_proposal(proposal_args.into()).await;
    assert_matches!(res, Ok(val) if val == ConsensusProposalCommitment::default());
}

#[tokio::test]
async fn interrupt_proposal() {
    let (mut proposal_args, _content_sender) = create_proposal_validate_arguments();
    // Interrupted proposals call validate_block and send Abort
    proposal_args.deps.batcher.expect_validate_block().times(1).returning(|_| Ok(()));
    proposal_args
        .deps
        .batcher
        .expect_start_height()
        .withf(|input| input.height == BlockNumber(0))
        .return_const(Ok(()));
    proposal_args.deps.batcher.expect_send_proposal_content().times(1).returning(|input| {
        assert!(matches!(input.content, SendProposalContent::Abort));
        Ok(SendProposalContentResponse { response: ProposalStatus::Processing })
    });

    // Interrupt the proposal.
    proposal_args.cancel_token.cancel();

    let res = validate_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(ValidateProposalError::ProposalInterrupted(_))));
}

#[tokio::test]
async fn validation_timeout() {
    let (mut proposal_args, _content_sender) = create_proposal_validate_arguments();
    // Timed out proposals call validate_block and send Abort
    proposal_args.deps.batcher.expect_validate_block().times(1).returning(|_| Ok(()));
    proposal_args
        .deps
        .batcher
        .expect_start_height()
        .withf(|input| input.height == BlockNumber(0))
        .return_const(Ok(()));
    proposal_args.deps.batcher.expect_send_proposal_content().times(1).returning(|input| {
        assert!(matches!(input.content, SendProposalContent::Abort));
        Ok(SendProposalContentResponse { response: ProposalStatus::Processing })
    });

    // Set a very short timeout to trigger a timeout error.
    proposal_args.timeout = Duration::from_micros(1);

    let res = validate_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(ValidateProposalError::ValidationTimeout(_))));
}

#[tokio::test]
async fn invalid_block_info() {
    let (mut proposal_args, mut content_sender) = create_proposal_validate_arguments();

    proposal_args.init.l2_gas_price_fri =
        GasPrice(proposal_args.block_info_validation.l2_gas_price_fri.0 + 1);
    content_sender.send(SignedProposalPart::init(proposal_args.init.clone())).await.unwrap();

    let res = validate_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(ValidateProposalError::InvalidBlockInfo(_, _, _))));
}

#[tokio::test]
async fn validate_block_fail() {
    let (mut proposal_args, _content_sender) = create_proposal_validate_arguments();
    // Setup batcher to return an error when validating the block.
    proposal_args.deps.batcher.expect_validate_block().returning(|_| {
        Err(BatcherClientError::ClientError(ClientError::CommunicationFailure("".to_string())))
    });

    let res = validate_proposal(proposal_args.into()).await;
    assert_matches!(res, Err(ValidateProposalError::Batcher(msg,_ ))
        if msg.contains("Failed to initiate validate proposal"));
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
    let received_fin = ConsensusProposalCommitment::default();
    content_sender
        .send(SignedProposalPart::fin(ProposalFin {
            proposal_commitment: received_fin,
            executed_transaction_count: n_executed.try_into().unwrap(),
            commitment_parts: None,
        }))
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
    content_sender
        .send(SignedProposalPart::fin(ProposalFin {
            proposal_commitment: ConsensusProposalCommitment::default(),
            executed_transaction_count: n_executed.try_into().unwrap(),
            commitment_parts: None,
        }))
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
