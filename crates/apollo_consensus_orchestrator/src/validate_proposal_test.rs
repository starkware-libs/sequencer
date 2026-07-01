use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollo_batcher_types::batcher_types::{
    FinishProposalInput,
    FinishProposalStatus,
    FinishedProposalInfo,
    FinishedProposalInfoWithoutParent,
    ProposalCommitment,
    ProposalId,
    SendTxsForProposalStatus,
};
use apollo_batcher_types::communication::BatcherClientError;
use apollo_consensus_orchestrator_config::config::ContextConfig;
use apollo_infra::component_client::ClientError;
use apollo_protobuf::consensus::{
    ProposalCommitment as ConsensusProposalCommitment,
    ProposalFin,
    ProposalInit,
    ProposalPart,
    TransactionBatch,
};
use apollo_versioned_constants::VersionedConstants;
use assert_matches::assert_matches;
use futures::channel::mpsc;
use futures::SinkExt;
use rstest::rstest;
use starknet_api::block::{BlockNumber, GasPrice, StarknetVersion};
use starknet_api::block_hash::block_hash_calculator::{BlockHeaderCommitments, PartialBlockHash};
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::execution_resources::GasAmount;
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use starknet_types_core::felt::Felt;
use tokio_util::sync::CancellationToken;

use crate::dynamic_gas_price::{proposal_commitment_from, PPT_DENOMINATOR};
use crate::sequencer_consensus_context::BuiltProposals;

fn fee_proposal_margin_ppt() -> u128 {
    VersionedConstants::latest_constants().fee_proposal_margin_ppt
}
use crate::test_utils::{
    create_test_and_network_deps,
    proposal_init,
    SetupDepsArgs,
    TestDeps,
    CHANNEL_SIZE,
    TIMEOUT,
    TX_BATCH,
};
use crate::utils::{expected_version_constant_commitment, make_gas_price_params, GasPriceParams};

/// The default-test proposal commitment that the validator computes when:
/// - the batcher returns `partial_block_hash == StarkHash::ZERO` (test default), and
/// - the init carries `fee_proposal_fri == Some(8 gwei)` (test default per `proposal_init`).
fn test_validate_expected_commitment() -> ConsensusProposalCommitment {
    proposal_commitment_from(PartialBlockHash::default(), Some(GasPrice(8_000_000_000)))
}
use crate::validate_proposal::{
    is_proposal_init_valid,
    validate_proposal,
    within_margin,
    ProposalInitValidation,
    ProposalValidateArguments,
    ValidateProposalError,
};

struct TestProposalValidateArguments {
    pub deps: TestDeps,
    pub init: ProposalInit,
    pub proposal_init_validation: ProposalInitValidation,
    pub proposal_id: ProposalId,
    pub timeout: Duration,
    pub batcher_timeout_margin: Duration,
    pub valid_proposals: Arc<Mutex<BuiltProposals>>,
    pub content_receiver: mpsc::Receiver<ProposalPart>,
    pub gas_price_params: GasPriceParams,
    pub cancel_token: CancellationToken,
    pub compare_retrospective_block_hash: bool,
}

impl From<TestProposalValidateArguments> for ProposalValidateArguments {
    fn from(args: TestProposalValidateArguments) -> Self {
        ProposalValidateArguments {
            deps: args.deps.into(),
            init: args.init,
            proposal_init_validation: args.proposal_init_validation,
            proposal_id: args.proposal_id,
            timeout: args.timeout,
            batcher_timeout_margin: args.batcher_timeout_margin,
            valid_proposals: args.valid_proposals,
            content_receiver: args.content_receiver,
            gas_price_params: args.gas_price_params,
            cancel_token: args.cancel_token,
            compare_retrospective_block_hash: args.compare_retrospective_block_hash,
        }
    }
}

fn create_proposal_validate_arguments()
-> (TestProposalValidateArguments, mpsc::Sender<ProposalPart>) {
    let (mut deps, _) = create_test_and_network_deps();
    deps.setup_default_expectations();
    let init = proposal_init(BlockNumber(0), 0);
    let proposal_init_validation = ProposalInitValidation {
        height: BlockNumber(0),
        block_timestamp_window_seconds: 60,
        previous_proposal_init: None,
        l1_da_mode: L1DataAvailabilityMode::Blob,
        l2_gas_price_fri: VersionedConstants::latest_constants().min_gas_price,
        starknet_version: StarknetVersion::LATEST,
        fee_actual: None,
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
            proposal_init_validation,
            proposal_id,
            timeout,
            batcher_timeout_margin,
            valid_proposals,
            content_receiver,
            gas_price_params,
            cancel_token,
            compare_retrospective_block_hash: true,
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
    proposal_args.deps.batcher.expect_finish_proposal().times(1).returning(|input| {
        assert_eq!(input.final_n_executed_txs, 0);
        Ok(FinishProposalStatus::Finished(FinishedProposalInfo {
            artifact: FinishedProposalInfoWithoutParent {
                proposal_commitment: ProposalCommitment::default(),
                final_n_executed_txs: 0,
                block_header_commitments: BlockHeaderCommitments::default(),
                l2_gas_used: GasAmount::default(),
            },
            parent_proposal_commitment: None,
        }))
    });

    // Send an empty proposal.
    let expected = test_validate_expected_commitment();
    content_sender
        .send(ProposalPart::Fin(ProposalFin {
            proposal_commitment: expected,
            executed_transaction_count: 0,
            fin_payload: None,
        }))
        .await
        .unwrap();

    let res = validate_proposal(proposal_args.into()).await;
    assert_matches!(res, Ok(val) if val == expected);
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
        .send(ProposalPart::Transactions(TransactionBatch { transactions: TX_BATCH.clone() }))
        .await
        .unwrap();
    let expected = test_validate_expected_commitment();
    content_sender
        .send(ProposalPart::Fin(ProposalFin {
            proposal_commitment: expected,
            executed_transaction_count: n_executed_txs_count.try_into().unwrap(),
            fin_payload: None,
        }))
        .await
        .unwrap();

    let res = validate_proposal(proposal_args.into()).await;
    assert_matches!(res, Ok(val) if val == expected);
}

#[tokio::test]
async fn fin_with_inflated_executed_tx_count_is_rejected() {
    let (mut proposal_args, mut content_sender) = create_proposal_validate_arguments();
    proposal_args.deps.setup_default_expectations();
    proposal_args.deps.batcher.expect_validate_block().times(1).returning(|_| Ok(()));
    proposal_args
        .deps
        .batcher
        .expect_start_height()
        .withf(|input| input.height == BlockNumber(0))
        .return_const(Ok(()));
    proposal_args
        .deps
        .batcher
        .expect_send_txs_for_proposal()
        .times(1)
        .returning(|_| Ok(SendTxsForProposalStatus::Processing));
    // The inflated count must be rejected before reaching the batcher: `finish_proposal` is
    // never called, and the in-progress proposal is aborted instead.
    proposal_args.deps.batcher.expect_finish_proposal().times(0);
    proposal_args.deps.batcher.expect_abort_proposal().times(1).returning(|_| Ok(()));

    // Stream TX_BATCH (3 txs), then claim one more was executed than was received.
    content_sender
        .send(ProposalPart::Transactions(TransactionBatch { transactions: TX_BATCH.clone() }))
        .await
        .unwrap();
    let inflated_count = (TX_BATCH.len() + 1).try_into().unwrap();
    content_sender
        .send(ProposalPart::Fin(ProposalFin {
            proposal_commitment: ConsensusProposalCommitment::default(),
            executed_transaction_count: inflated_count,
            fin_payload: None,
        }))
        .await
        .unwrap();

    let res = validate_proposal(proposal_args.into()).await;
    assert_matches!(res, Err(ValidateProposalError::ProposalPartFailed(_, _)));
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
    proposal_args.deps.batcher.expect_abort_proposal().times(1).returning(|_| Ok(()));

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
    proposal_args.deps.batcher.expect_abort_proposal().times(1).returning(|_| Ok(()));

    // Set a very short timeout to trigger a timeout error.
    proposal_args.timeout = Duration::from_micros(1);

    let res = validate_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(ValidateProposalError::ValidationTimeout(_))));
}

#[tokio::test]
async fn invalid_proposal_init() {
    let (mut proposal_args, mut content_sender) = create_proposal_validate_arguments();

    proposal_args.init.l2_gas_price_fri =
        GasPrice(proposal_args.proposal_init_validation.l2_gas_price_fri.0 + 1);
    content_sender.send(ProposalPart::Init(proposal_args.init.clone())).await.unwrap();

    let res = validate_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(ValidateProposalError::InvalidProposalInit(_, _, _))));
}

#[derive(Copy, Clone, Debug)]
enum L1GasPriceField {
    GasPriceFri,
    DataGasPriceFri,
    GasPriceWei,
    DataGasPriceWei,
}

#[rstest]
#[case::l1_gas_price_fri(L1GasPriceField::GasPriceFri)]
#[case::l1_data_gas_price_fri(L1GasPriceField::DataGasPriceFri)]
#[case::l1_gas_price_wei(L1GasPriceField::GasPriceWei)]
#[case::l1_data_gas_price_wei(L1GasPriceField::DataGasPriceWei)]
#[tokio::test]
async fn rejects_proposal_init_l1_gas_price_out_of_margin(#[case] field: L1GasPriceField) {
    let (proposal_args, _content_sender) = create_proposal_validate_arguments();
    let TestProposalValidateArguments {
        deps,
        mut init,
        proposal_init_validation,
        gas_price_params,
        ..
    } = proposal_args;
    // Push the targeted L1 gas-price field an order of magnitude above the validator's
    // reference value so it falls well outside the 10% margin enforced via
    // VersionedConstants::l1_gas_price_margin_percent.
    let target = match field {
        L1GasPriceField::GasPriceFri => &mut init.l1_gas_price_fri,
        L1GasPriceField::DataGasPriceFri => &mut init.l1_data_gas_price_fri,
        L1GasPriceField::GasPriceWei => &mut init.l1_gas_price_wei,
        L1GasPriceField::DataGasPriceWei => &mut init.l1_data_gas_price_wei,
    };
    *target = GasPrice(target.0.saturating_mul(10));

    let res = is_proposal_init_valid(
        &proposal_init_validation,
        &init,
        deps.clock.as_ref(),
        Arc::new(deps.l1_gas_price_provider),
        &gas_price_params,
    )
    .await;

    assert_matches!(
        res,
        Err(ValidateProposalError::InvalidProposalInit(_, _, ref msg))
            if msg.contains("L1 gas price mismatch")
    );
}

// fee_actual = 8 gwei; bounds are derived in-line so they stay correct if the fee_proposal
// constants change. upper = fee_actual * (PPT + MARGIN) / PPT;
// lower = fee_actual * PPT / (PPT + MARGIN) (integer-truncated).
const FEE_ACTUAL_FRI: u128 = 8_000_000_000;
#[rstest]
#[case::at_fee_actual(FEE_ACTUAL_FRI, true)]
#[case::upper_bound_inclusive(
    FEE_ACTUAL_FRI * (PPT_DENOMINATOR + fee_proposal_margin_ppt()) / PPT_DENOMINATOR,
    true,
)]
#[case::lower_bound_inclusive(
    FEE_ACTUAL_FRI * PPT_DENOMINATOR / (PPT_DENOMINATOR + fee_proposal_margin_ppt()),
    true,
)]
#[case::above_upper_bound(
    FEE_ACTUAL_FRI * (PPT_DENOMINATOR + fee_proposal_margin_ppt()) / PPT_DENOMINATOR + 1,
    false,
)]
#[case::below_lower_bound(
    FEE_ACTUAL_FRI * PPT_DENOMINATOR / (PPT_DENOMINATOR + fee_proposal_margin_ppt()) - 1,
    false,
)]
#[tokio::test]
async fn fee_proposal_within_margin_of_fee_actual(
    #[case] fee_proposal_fri: u128,
    #[case] should_accept: bool,
) {
    let (proposal_args, _content_sender) = create_proposal_validate_arguments();
    let TestProposalValidateArguments {
        deps,
        mut init,
        mut proposal_init_validation,
        gas_price_params,
        ..
    } = proposal_args;
    proposal_init_validation.fee_actual = Some(GasPrice(8_000_000_000));
    init.fee_proposal_fri = Some(GasPrice(fee_proposal_fri));

    let res = is_proposal_init_valid(
        &proposal_init_validation,
        &init,
        deps.clock.as_ref(),
        Arc::new(deps.l1_gas_price_provider),
        &gas_price_params,
    )
    .await;

    if should_accept {
        assert!(res.is_ok(), "expected accept, got {res:?}");
    } else {
        assert_matches!(
            res,
            Err(ValidateProposalError::InvalidProposalInit(_, _, ref msg))
                if msg.contains("Fee proposal out of bounds")
        );
    }
}

// The proposer (`build_proposal`) and the validator both read the commitment from
// `expected_version_constant_commitment()`, so by construction they cannot drift: the accept case
// feeds the validator exactly the value the proposer emits, and the reject case proves the
// validator enforces the match rather than ignoring the field (the regression that would recreate
// the "populated but unvalidated" half-state).
#[rstest]
#[case::accepts_value_the_proposer_emits(expected_version_constant_commitment(), true)]
#[case::rejects_any_other_value(expected_version_constant_commitment() + Felt::ONE, false)]
#[tokio::test]
async fn validates_version_constant_commitment(
    #[case] version_constant_commitment: Felt,
    #[case] should_accept: bool,
) {
    let (proposal_args, _content_sender) = create_proposal_validate_arguments();
    let TestProposalValidateArguments {
        deps,
        mut init,
        proposal_init_validation,
        gas_price_params,
        ..
    } = proposal_args;
    init.version_constant_commitment = version_constant_commitment;

    let res = is_proposal_init_valid(
        &proposal_init_validation,
        &init,
        deps.clock.as_ref(),
        Arc::new(deps.l1_gas_price_provider),
        &gas_price_params,
    )
    .await;

    if should_accept {
        assert!(res.is_ok(), "expected accept, got {res:?}");
    } else {
        assert_matches!(
            res,
            Err(ValidateProposalError::InvalidProposalInit(_, _, ref msg))
                if msg.contains("version_constant_commitment mismatch")
        );
    }
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
    let built_block = PartialBlockHash(Felt::ONE);
    proposal_args
        .deps
        .batcher
        .expect_finish_proposal()
        .withf(move |input: &FinishProposalInput| {
            input.proposal_id == proposal_args.proposal_id
                && input.final_n_executed_txs == n_executed
        })
        .returning(move |_| {
            Ok(FinishProposalStatus::Finished(FinishedProposalInfo {
                artifact: FinishedProposalInfoWithoutParent {
                    proposal_commitment: ProposalCommitment { partial_block_hash: built_block },
                    final_n_executed_txs: n_executed,
                    block_header_commitments: BlockHeaderCommitments::default(),
                    l2_gas_used: GasAmount::default(),
                },
                parent_proposal_commitment: None,
            }))
        });
    let received_fin = ConsensusProposalCommitment::default();
    content_sender
        .send(ProposalPart::Fin(ProposalFin {
            proposal_commitment: received_fin,
            executed_transaction_count: n_executed.try_into().unwrap(),
            fin_payload: None,
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
        .expect_finish_proposal()
        .withf(move |input: &FinishProposalInput| {
            input.proposal_id == proposal_args.proposal_id
                && input.final_n_executed_txs == n_executed
        })
        .returning(|_| Ok(FinishProposalStatus::InvalidProposal("test error".to_string())));
    content_sender
        .send(ProposalPart::Fin(ProposalFin {
            proposal_commitment: ConsensusProposalCommitment::default(),
            executed_transaction_count: n_executed.try_into().unwrap(),
            fin_payload: None,
        }))
        .await
        .unwrap();

    let res = validate_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(ValidateProposalError::InvalidProposal(_))));
}

#[tokio::test]
async fn invalid_starknet_version() {
    let (mut proposal_args, _content_sender) = create_proposal_validate_arguments();
    // Proposer sends a starknet_version that doesn't match what the validator expects.
    proposal_args.init.starknet_version = StarknetVersion::V0_13_4;

    let res = validate_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(ValidateProposalError::InvalidProposalInit(_, _, ref msg))
        if msg.contains("starknet_version mismatch")));
}

// Cases are (proposed, reference, margin_percent, expected). The band is anchored to `reference`.
#[rstest]
#[case::big_number_in_margin(1000, 1050, 10, true)]
#[case::big_number_out_of_margin(1000, 1150, 10, false)]
#[case::small_number_in_margin(9, 10, 10, true)]
#[case::small_number_out_of_margin(9, 11, 10, false)]
#[case::identical_numbers(12345, 12345, 1, true)]
// Reference-anchored margin is 1000*10/100 = 100 < diff 111, so this is rejected; a
// proposed-anchored margin would be 1111*10/100 = 111 and would wrongly accept it.
#[case::inflated_proposed_rejected(1111, 1000, 10, false)]
#[case::upper_bound_inclusive(1100, 1000, 10, true)]
#[case::lower_bound_inclusive(900, 1000, 10, true)]
#[case::deflated_proposed_rejected(889, 1000, 10, false)]
// Equal values hit the abs_diff early return, before the (saturating) margin multiply.
#[case::large_identical_no_overflow(u128::MAX, u128::MAX, 10, true)]
#[case::large_within_no_overflow(u128::MAX - 5, u128::MAX, 10, true)]
fn test_within_margin(
    #[case] proposed: u128,
    #[case] reference: u128,
    #[case] margin: u128,
    #[case] expected: bool,
) {
    assert_eq!(within_margin(GasPrice(proposed), GasPrice(reference), margin), expected);
}

// A far-out proposed against a huge reference must reject without overflowing the margin multiply.
#[test]
fn within_margin_large_reference_does_not_overflow() {
    assert!(!within_margin(GasPrice(1), GasPrice(u128::MAX), 10));
}
