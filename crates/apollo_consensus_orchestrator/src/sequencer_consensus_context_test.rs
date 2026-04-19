use std::future::ready;
use std::sync::Arc;
use std::time::Duration;

use apollo_batcher_types::batcher_types::{
    CentralObjects,
    DecisionReachedResponse,
    FinishProposalStatus,
    FinishedProposalInfo,
    FinishedProposalInfoWithoutParent,
    ProposalCommitment as BatcherProposalCommitment,
    SendTxsForProposalStatus,
};
use apollo_batcher_types::communication::BatcherClientError;
use apollo_batcher_types::errors::BatcherError;
use apollo_config_manager_types::communication::MockConfigManagerClient;
use apollo_consensus::types::{ConsensusContext, Round};
use apollo_consensus_orchestrator_config::config::{
    ContextConfig,
    ContextDynamicConfig,
    ContextStaticConfig,
    PricePerHeight,
};
use apollo_l1_gas_price_types::errors::{
    L1GasPriceClientError,
    L1GasPriceProviderError,
    PriceOracleClientError,
};
use apollo_l1_gas_price_types::{MockL1GasPriceProviderClient, PriceInfo};
use apollo_protobuf::consensus::{
    BuildParam,
    CommitmentParts,
    L2GasInfo,
    ProposalCommitment,
    ProposalFin,
    ProposalFinPayload,
    ProposalPart,
    TransactionBatch,
};
use apollo_state_sync_types::state_sync_types::SyncBlock;
use apollo_time::time::MockClock;
use apollo_versioned_constants::VersionedConstants;
use chrono::{TimeZone, Utc};
use futures::channel::mpsc;
use futures::channel::oneshot::Canceled;
use futures::future::pending;
use futures::{FutureExt, SinkExt, StreamExt};
use metrics_exporter_prometheus::PrometheusBuilder;
use rstest::rstest;
use starknet_api::block::{
    BlockHash,
    BlockNumber,
    GasPrice,
    TEMP_ETH_BLOB_GAS_FEE_IN_WEI,
    TEMP_ETH_GAS_FEE_IN_WEI,
    WEI_PER_ETH,
};
use starknet_api::block_hash::block_hash_calculator::BlockHeaderCommitments;
use starknet_api::execution_resources::GasAmount;
use starknet_api::hash::StarkHash;
use starknet_api::state::ThinStateDiff;
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;

use crate::cende::{MockCendeContext, N_BLOCK_HASHES_BACK_IN_BLOB};
use crate::metrics::CONSENSUS_L2_GAS_PRICE;
use crate::sequencer_consensus_context::{
    SequencerConsensusContext,
    SequencerConsensusContextDeps,
};
use crate::test_utils::{
    create_test_and_network_deps,
    proposal_init,
    send_proposal_to_validator_context,
    SetupDepsArgs,
    CHAIN_ID,
    CHANNEL_SIZE,
    ETH_TO_FRI_RATE,
    INTERNAL_TX_BATCH,
    PARTIAL_BLOCK_HASH,
    TIMEOUT,
    TX_BATCH,
};

/// Expected L2GasInfo when build_proposal runs with test defaults (min gas price, l2_gas_used 0).
fn expected_l2_gas_info_for_build_proposal_defaults() -> L2GasInfo {
    L2GasInfo {
        next_l2_gas_price_fri: VersionedConstants::latest_constants().min_gas_price,
        l2_gas_used: GasAmount(0),
    }
}
use crate::utils::{apply_fee_transformations, make_gas_price_params};

const TEST_PROPOSAL_COMMITMENT: ProposalCommitment = ProposalCommitment(PARTIAL_BLOCK_HASH.0);
const HEIGHT_0: BlockNumber = BlockNumber(0);
const HEIGHT_1: BlockNumber = BlockNumber(1);

// Use heights < 10 to avoid triggering the height-10 block-hash mapping code path (not tested
// here). Use non-zero height because height 0 always skips the write without querying the recorder.
const HEIGHT_FOR_WRITE_TESTS: BlockNumber = BlockNumber(8);

const ROUND_0: Round = 0;
const ROUND_1: Round = 1;

#[tokio::test]
async fn cancelled_proposal_aborts() {
    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_default_expectations();

    deps.batcher.expect_propose_block().times(1).return_const(Ok(()));
    deps.batcher.expect_start_height().times(1).return_const(Ok(()));

    let mut context = deps.build_context();
    let fin_receiver = context.build_proposal(BuildParam::default(), TIMEOUT).await.unwrap();

    // Now we intrrupt the proposal and verify that the fin_receiever is dropped.
    context.set_height_and_round(HEIGHT_0, ROUND_1).await.unwrap();

    assert_eq!(fin_receiver.await, Err(Canceled));
}

#[tokio::test]
async fn validate_proposal_success() {
    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_deps_for_validate(SetupDepsArgs::default());
    let mut context = deps.build_context();

    // Initialize the context for a specific height, starting with round 0.
    context.set_height_and_round(HEIGHT_0, ROUND_0).await.unwrap();
    let content_receiver = send_proposal_to_validator_context(&mut context).await;
    let fin_receiver = context
        .validate_proposal(proposal_init(HEIGHT_0, ROUND_0), TIMEOUT, content_receiver)
        .await;
    assert_eq!(fin_receiver.await.unwrap(), TEST_PROPOSAL_COMMITMENT);
}

#[rstest]
#[case::execute_all_txs(true)]
#[case::dont_execute_last_tx(false)]
#[tokio::test]
async fn validate_then_repropose(#[case] execute_all_txs: bool) {
    // Receive a proposal. Then re-retrieve it.
    let (mut deps, mut network) = create_test_and_network_deps();
    let executed_transactions = match execute_all_txs {
        true => TX_BATCH.to_vec(),
        false => TX_BATCH.iter().take(TX_BATCH.len() - 1).cloned().collect(),
    };
    let n_executed_txs_count = executed_transactions.len();
    deps.setup_deps_for_validate(SetupDepsArgs { n_executed_txs_count, ..Default::default() });

    const TIMESTAMP: u64 = 123456;
    deps.batcher
        .expect_decision_reached()
        .times(1)
        .return_once(|_| Ok(DecisionReachedResponse::default()));
    deps.state_sync_client.expect_add_new_block().times(1).return_once(move |proposal_init| {
        assert_eq!(
            proposal_init.block_header_without_hash.timestamp.0, TIMESTAMP,
            "add_new_block should be called with timestamp from initial proposal (unchanged \
             during reproposal for commitment consistency)"
        );
        Ok(())
    });
    deps.cende_ambassador.expect_prepare_blob_for_next_height().return_once(|_| Ok(()));

    let mut context = deps.build_context();

    // Initialize the context for a specific height, starting with round 0.
    context.set_height_and_round(HEIGHT_0, ROUND_0).await.unwrap();

    // Receive a valid proposal. Use timestamp matching MockClock so validation passes.
    let (mut content_sender, content_receiver) =
        mpsc::channel(context.config.static_config.proposal_buffer_size);
    let mut init = proposal_init(HEIGHT_0, ROUND_0);
    init.timestamp = TIMESTAMP;
    let transactions =
        ProposalPart::Transactions(TransactionBatch { transactions: TX_BATCH.to_vec() });
    content_sender.send(transactions.clone()).await.unwrap();
    let fin = ProposalPart::Fin(ProposalFin {
        proposal_commitment: TEST_PROPOSAL_COMMITMENT,
        executed_transaction_count: n_executed_txs_count.try_into().unwrap(),
        fin_payload: Some(ProposalFinPayload {
            commitment_parts: CommitmentParts::default(),
            l2_gas_info: expected_l2_gas_info_for_build_proposal_defaults(),
        }),
    });
    content_sender.send(fin.clone()).await.unwrap();
    let fin_receiver = context.validate_proposal(init.clone(), TIMEOUT, content_receiver).await;
    content_sender.close_channel();
    assert_eq!(fin_receiver.await.unwrap(), TEST_PROPOSAL_COMMITMENT);

    let build_param =
        BuildParam { round: ROUND_1, valid_round: Some(ROUND_0), ..Default::default() };
    context.repropose(TEST_PROPOSAL_COMMITMENT, build_param).await;
    let (_, mut receiver) = network.outbound_proposal_receiver.next().await.unwrap();
    // Reproposal sends init with updated round, proposer, valid_round.
    let mut expected_init = init;
    expected_init.round = ROUND_1;
    expected_init.proposer = build_param.proposer;
    expected_init.valid_round = build_param.valid_round;
    assert_eq!(receiver.next().await.unwrap(), ProposalPart::Init(expected_init));
    assert_eq!(
        receiver.next().await.unwrap(),
        ProposalPart::Transactions(TransactionBatch { transactions: executed_transactions })
    );
    assert_eq!(receiver.next().await.unwrap(), fin);
    assert!(receiver.next().await.is_none());

    // Verify decision_reached uses the updated init (from reproposal round) for finalize.
    context.decision_reached(HEIGHT_0, ROUND_1, TEST_PROPOSAL_COMMITMENT, false).await.unwrap();
}

#[tokio::test]
async fn validate_then_build_then_decision_reached_round_0_uses_round_0_init() {
    // Scenario: validate round 0 with init timestamp X, build round 1 with clock returning Y
    // (different from X), decision_reached(round 0). State sync must receive timestamp
    // X (from round 0 init), not Y (from round 1 build).
    let (mut deps, mut network) = create_test_and_network_deps();
    deps.setup_deps_for_validate(SetupDepsArgs::default());
    deps.setup_deps_for_build(SetupDepsArgs { expect_start_height: false, ..Default::default() });

    const TIMESTAMP_ROUND_0: u64 = 123456;
    const TIMESTAMP_ROUND_1: u64 = 789012; // Different from round 0

    let mut clock = MockClock::new();
    clock.expect_unix_now().return_const(TIMESTAMP_ROUND_1);
    clock
        .expect_now()
        .return_const(Utc.timestamp_opt(TIMESTAMP_ROUND_1.try_into().unwrap(), 0).unwrap());
    deps.clock = Arc::new(clock);

    deps.batcher
        .expect_decision_reached()
        .times(1)
        .return_once(|_| Ok(DecisionReachedResponse::default()));
    deps.state_sync_client.expect_add_new_block().times(1).return_once(move |sync_block| {
        assert_eq!(
            sync_block.block_header_without_hash.timestamp.0, TIMESTAMP_ROUND_0,
            "add_new_block should be called with timestamp from round 0 validation (X), not from \
             round 1 build (Y)"
        );
        Ok(())
    });
    deps.cende_ambassador.expect_prepare_blob_for_next_height().return_once(|_| Ok(()));

    let mut context = deps.build_context();

    // Round 0: validate with init timestamp TIMESTAMP_ROUND_0
    context.set_height_and_round(HEIGHT_0, ROUND_0).await.unwrap();
    let (mut content_sender, content_receiver) =
        mpsc::channel(context.config.static_config.proposal_buffer_size);
    let mut init = proposal_init(HEIGHT_0, ROUND_0);
    init.timestamp = TIMESTAMP_ROUND_0;
    let transactions =
        ProposalPart::Transactions(TransactionBatch { transactions: TX_BATCH.to_vec() });
    content_sender.send(transactions.clone()).await.unwrap();
    let fin = ProposalPart::Fin(ProposalFin {
        proposal_commitment: TEST_PROPOSAL_COMMITMENT,
        executed_transaction_count: TX_BATCH.len().try_into().unwrap(),
        fin_payload: None,
    });
    content_sender.send(fin.clone()).await.unwrap();
    let fin_receiver = context.validate_proposal(init, TIMEOUT, content_receiver).await;
    content_sender.close_channel();
    assert_eq!(fin_receiver.await.unwrap(), TEST_PROPOSAL_COMMITMENT);

    // Round 1: build - clock returns TIMESTAMP_ROUND_1 (different from TIMESTAMP_ROUND_0)
    let build_param = BuildParam { round: ROUND_1, ..Default::default() };
    let fin_receiver = context.build_proposal(build_param, TIMEOUT).await.unwrap();

    // Build sends proposal with TIMESTAMP_ROUND_1 from clock
    let (_, mut receiver) = network.outbound_proposal_receiver.next().await.unwrap();
    let part = receiver.next().await.unwrap();
    let ProposalPart::Init(build_init) = part else {
        panic!("Expected Init part");
    };
    assert_eq!(build_init.timestamp, TIMESTAMP_ROUND_1);
    let _txs = receiver.next().await.unwrap();
    let _fin = receiver.next().await.unwrap();
    assert_eq!(fin_receiver.await.unwrap(), TEST_PROPOSAL_COMMITMENT);

    // Decision reached for round 0 - state_sync should receive TIMESTAMP_ROUND_0
    context.decision_reached(HEIGHT_0, ROUND_0, TEST_PROPOSAL_COMMITMENT, false).await.unwrap();
}

#[tokio::test]
async fn proposals_from_different_rounds() {
    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_deps_for_validate(SetupDepsArgs::default());
    let mut context = deps.build_context();
    // Initialize the context for a specific height, starting with round 0.
    context.set_height_and_round(HEIGHT_0, ROUND_0).await.unwrap();
    context.set_height_and_round(HEIGHT_0, ROUND_1).await.unwrap();

    // Proposal parts sent in the proposals.
    let prop_part_txs =
        ProposalPart::Transactions(TransactionBatch { transactions: TX_BATCH.to_vec() });
    let prop_part_fin = ProposalPart::Fin(ProposalFin {
        proposal_commitment: TEST_PROPOSAL_COMMITMENT,
        executed_transaction_count: INTERNAL_TX_BATCH.len().try_into().unwrap(),
        fin_payload: Some(ProposalFinPayload::default()),
    });

    // The proposal from the past round is ignored.
    let (mut content_sender, content_receiver) =
        mpsc::channel(context.config.static_config.proposal_buffer_size);
    content_sender.send(prop_part_txs.clone()).await.unwrap();

    let fin_receiver_past_round = context
        .validate_proposal(proposal_init(HEIGHT_0, ROUND_0), TIMEOUT, content_receiver)
        .await;
    // No fin was sent, channel remains open.
    assert!(fin_receiver_past_round.await.is_err());

    // The proposal from the current round should be validated.
    let (mut content_sender, content_receiver) =
        mpsc::channel(context.config.static_config.proposal_buffer_size);
    content_sender.send(prop_part_txs.clone()).await.unwrap();
    content_sender.send(prop_part_fin.clone()).await.unwrap();
    let fin_receiver_curr_round = context
        .validate_proposal(proposal_init(HEIGHT_0, ROUND_1), TIMEOUT, content_receiver)
        .await;
    assert_eq!(fin_receiver_curr_round.await.unwrap(), TEST_PROPOSAL_COMMITMENT);

    // The proposal from the future round should not be processed.
    let (mut content_sender, content_receiver) =
        mpsc::channel(context.config.static_config.proposal_buffer_size);
    content_sender.send(prop_part_txs.clone()).await.unwrap();
    content_sender.send(prop_part_fin.clone()).await.unwrap();
    let fin_receiver_future_round =
        context.validate_proposal(proposal_init(HEIGHT_0, 2), TIMEOUT, content_receiver).await;
    content_sender.close_channel();
    // Even with sending fin and closing the channel.
    assert!(fin_receiver_future_round.now_or_never().is_none());
}

#[tokio::test]
async fn interrupt_active_proposal() {
    let (mut deps, _network) = create_test_and_network_deps();
    // This test validates two proposals: round 0 (interrupted) and round 1 (successful)
    deps.setup_default_expectations();

    // Expect 2 validate_block calls (one for each round)
    deps.batcher.expect_validate_block().times(2).returning(|_| Ok(()));
    deps.batcher.expect_start_height().withf(|input| input.height == HEIGHT_0).return_const(Ok(()));

    // Round 0: Will be interrupted and send Abort
    deps.batcher.expect_abort_proposal().times(1).returning(|_| Ok(()));

    // Round 1: Will send Txs then Finish
    deps.batcher.expect_send_txs_for_proposal().times(1).returning(|input| {
        let txs = input.txs;
        assert_eq!(txs, *INTERNAL_TX_BATCH);
        Ok(SendTxsForProposalStatus::Processing)
    });
    deps.batcher.expect_finish_proposal().times(1).returning(|input| {
        assert_eq!(input.final_n_executed_txs, INTERNAL_TX_BATCH.len());
        Ok(FinishProposalStatus::Finished(FinishedProposalInfo {
            artifact: FinishedProposalInfoWithoutParent {
                proposal_commitment: BatcherProposalCommitment {
                    partial_block_hash: PARTIAL_BLOCK_HASH,
                },
                final_n_executed_txs: 0,
                block_header_commitments: BlockHeaderCommitments::default(),
                l2_gas_used: GasAmount::default(),
            },
            parent_proposal_commitment: None,
        }))
    });

    let mut context = deps.build_context();
    // Initialize the context for a specific height, starting with round 0.
    context.set_height_and_round(HEIGHT_0, ROUND_0).await.unwrap();

    // Keep the sender open, as closing it or sending Fin would cause the validate to complete
    // without needing interrupt.
    let (mut _content_sender_0, content_receiver) =
        mpsc::channel(context.config.static_config.proposal_buffer_size);
    let fin_receiver_0 = context
        .validate_proposal(proposal_init(HEIGHT_0, ROUND_0), TIMEOUT, content_receiver)
        .await;

    let content_receiver = send_proposal_to_validator_context(&mut context).await;
    let fin_receiver_1 = context
        .validate_proposal(proposal_init(HEIGHT_0, ROUND_1), TIMEOUT, content_receiver)
        .await;
    // Move the context to the next round.
    context.set_height_and_round(HEIGHT_0, ROUND_1).await.unwrap();

    // Interrupt active proposal.
    assert!(fin_receiver_0.await.is_err());
    assert_eq!(fin_receiver_1.await.unwrap(), TEST_PROPOSAL_COMMITMENT);
}

#[tokio::test]
async fn build_proposal() {
    let before: u64 =
        chrono::Utc::now().timestamp().try_into().expect("Timestamp conversion failed");
    let (mut deps, mut network) = create_test_and_network_deps();
    deps.setup_deps_for_build(SetupDepsArgs::default());
    let mut context = deps.build_context();
    let fin_receiver = context.build_proposal(BuildParam::default(), TIMEOUT).await.unwrap();
    // Test proposal parts.
    let (_, mut receiver) = network.outbound_proposal_receiver.next().await.unwrap();
    let part = receiver.next().await.unwrap();
    let after: u64 =
        chrono::Utc::now().timestamp().try_into().expect("Timestamp conversion failed");
    let ProposalPart::Init(info) = part else {
        panic!("Expected ProposalPart::Init");
    };
    assert!(info.timestamp >= before && info.timestamp <= after);
    assert_eq!(
        receiver.next().await.unwrap(),
        ProposalPart::Transactions(TransactionBatch { transactions: TX_BATCH.to_vec() })
    );
    assert_eq!(
        receiver.next().await.unwrap(),
        ProposalPart::Fin(ProposalFin {
            proposal_commitment: TEST_PROPOSAL_COMMITMENT,
            executed_transaction_count: INTERNAL_TX_BATCH.len().try_into().unwrap(),
            fin_payload: Some(ProposalFinPayload {
                commitment_parts: CommitmentParts::default(),
                l2_gas_info: expected_l2_gas_info_for_build_proposal_defaults(),
            }),
        })
    );
    assert!(receiver.next().await.is_none());
    assert_eq!(fin_receiver.await.unwrap(), TEST_PROPOSAL_COMMITMENT);
}

#[tokio::test]
async fn build_proposal_cende_failure() {
    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_deps_for_build(SetupDepsArgs {
        start_block_number: HEIGHT_FOR_WRITE_TESTS,
        ..Default::default()
    });
    let mut mock_cende_context = MockCendeContext::new();
    mock_cende_context
        .expect_write_prev_height_blob()
        .times(1)
        .return_once(|_height| tokio::spawn(ready(false)));
    deps.cende_ambassador = mock_cende_context;
    let mut context = deps.build_context();

    let fin_receiver = context
        .build_proposal(
            BuildParam { height: HEIGHT_FOR_WRITE_TESTS, ..Default::default() },
            TIMEOUT,
        )
        .await
        .unwrap();
    assert_eq!(fin_receiver.await, Err(Canceled));
}

#[tokio::test]
async fn build_proposal_cende_incomplete() {
    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_deps_for_build(SetupDepsArgs {
        start_block_number: HEIGHT_FOR_WRITE_TESTS,
        ..Default::default()
    });
    let mut mock_cende_context = MockCendeContext::new();
    mock_cende_context
        .expect_write_prev_height_blob()
        .times(1)
        .return_once(|_height| tokio::spawn(pending()));
    deps.cende_ambassador = mock_cende_context;
    let mut context = deps.build_context();

    let fin_receiver = context
        .build_proposal(
            BuildParam { height: HEIGHT_FOR_WRITE_TESTS, ..Default::default() },
            TIMEOUT,
        )
        .await
        .unwrap();
    assert_eq!(fin_receiver.await, Err(Canceled));
}

#[rstest]
#[case::proposer(true)]
#[case::validator(false)]
#[tokio::test]
async fn batcher_not_ready(#[case] proposer: bool) {
    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_default_expectations();
    deps.batcher.expect_start_height().times(1).return_const(Ok(()));
    if proposer {
        deps.batcher
            .expect_propose_block()
            .times(1)
            .return_const(Err(BatcherClientError::BatcherError(BatcherError::NotReady)));
    } else {
        deps.batcher
            .expect_validate_block()
            .times(1)
            .return_const(Err(BatcherClientError::BatcherError(BatcherError::NotReady)));
    }
    let mut context = deps.build_context();
    context.set_height_and_round(BlockNumber::default(), ROUND_0).await.unwrap();

    if proposer {
        let fin_receiver = context.build_proposal(BuildParam::default(), TIMEOUT).await.unwrap();
        assert_eq!(fin_receiver.await, Err(Canceled));
    } else {
        let (_content_sender, content_receiver) =
            mpsc::channel(context.config.static_config.proposal_buffer_size);

        let fin_receiver = context
            .validate_proposal(proposal_init(HEIGHT_0, ROUND_0), TIMEOUT, content_receiver)
            .await;
        assert_eq!(fin_receiver.await, Err(Canceled));
    }
}

#[rstest]
#[case::execute_all_txs(true)]
#[case::dont_execute_last_tx(false)]
#[tokio::test]
async fn propose_then_repropose(#[case] execute_all_txs: bool) {
    let (mut deps, mut network) = create_test_and_network_deps();
    let transactions = match execute_all_txs {
        true => TX_BATCH.to_vec(),
        false => TX_BATCH.iter().take(TX_BATCH.len() - 1).cloned().collect(),
    };
    deps.setup_deps_for_build(SetupDepsArgs {
        n_executed_txs_count: transactions.len(),
        ..Default::default()
    });

    const TIMESTAMP: u64 = 123456;
    let mut clock = MockClock::new();
    clock.expect_unix_now().return_const(TIMESTAMP);
    clock.expect_now().return_const(Utc.timestamp_opt(TIMESTAMP.try_into().unwrap(), 0).unwrap());
    deps.clock = Arc::new(clock);

    deps.batcher
        .expect_decision_reached()
        .times(1)
        .return_once(|_| Ok(DecisionReachedResponse::default()));
    deps.state_sync_client.expect_add_new_block().times(1).return_once(move |proposal_init| {
        assert_eq!(
            proposal_init.block_header_without_hash.timestamp.0, TIMESTAMP,
            "add_new_block should be called with timestamp from initial proposal (unchanged \
             during reproposal for commitment consistency)"
        );
        Ok(())
    });
    deps.cende_ambassador.expect_prepare_blob_for_next_height().return_once(|_| Ok(()));

    let mut context = deps.build_context();
    // Build proposal.
    let fin_receiver = context.build_proposal(BuildParam::default(), TIMEOUT).await.unwrap();
    let (_, mut receiver) = network.outbound_proposal_receiver.next().await.unwrap();
    // Receive the proposal parts.
    let ProposalPart::Init(original_init) = receiver.next().await.unwrap() else {
        panic!("Expected Init part");
    };
    let _txs = receiver.next().await.unwrap();
    let fin = receiver.next().await.unwrap();
    assert_eq!(fin_receiver.await.unwrap(), TEST_PROPOSAL_COMMITMENT);

    // Re-propose.
    let build_param =
        BuildParam { round: ROUND_1, valid_round: Some(ROUND_0), ..Default::default() };
    context.repropose(TEST_PROPOSAL_COMMITMENT, build_param).await;
    // Re-propose sends the same proposal content but with updated init (round, proposer,
    // valid_round).
    let (_, mut receiver) = network.outbound_proposal_receiver.next().await.unwrap();
    let mut expected_init = original_init;
    expected_init.round = ROUND_1;
    expected_init.proposer = build_param.proposer;
    expected_init.valid_round = build_param.valid_round;
    assert_eq!(receiver.next().await.unwrap(), ProposalPart::Init(expected_init));

    let reproposed_txs = ProposalPart::Transactions(TransactionBatch { transactions });
    assert_eq!(receiver.next().await.unwrap(), reproposed_txs);

    assert_eq!(receiver.next().await.unwrap(), fin);
    assert!(receiver.next().await.is_none());

    // Verify decision_reached uses the updated init (from reproposal round) for finalize.
    context.decision_reached(HEIGHT_0, ROUND_1, TEST_PROPOSAL_COMMITMENT, false).await.unwrap();
}

#[tokio::test]
async fn gas_price_fri_out_of_range() {
    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_default_expectations();

    deps.batcher
        .expect_start_height()
        .times(1)
        .withf(|input| input.height == HEIGHT_0)
        .return_const(Ok(()));
    let mut context = deps.build_context();
    context.set_height_and_round(HEIGHT_0, ROUND_0).await.unwrap();
    let (_content_sender, content_receiver) =
        mpsc::channel(context.config.static_config.proposal_buffer_size);
    // Receive a block info with l1_gas_price_fri that is outside the margin of error.
    let mut init_1 = proposal_init(HEIGHT_0, ROUND_0);
    init_1.l1_gas_price_fri = init_1.l1_gas_price_fri.checked_mul_u128(2).unwrap();
    // Use a large enough timeout to ensure fin_receiver was canceled due to invalid init,
    // not due to a timeout.
    let fin_receiver = context.validate_proposal(init_1, TIMEOUT * 100, content_receiver).await;
    assert_eq!(fin_receiver.await, Err(Canceled));

    // Do the same for data gas price.
    let (mut content_sender, content_receiver) =
        mpsc::channel(context.config.static_config.proposal_buffer_size);
    let mut init_2 = proposal_init(HEIGHT_0, ROUND_0);
    init_2.l1_data_gas_price_fri = init_2.l1_data_gas_price_fri.checked_mul_u128(2).unwrap();
    content_sender.send(ProposalPart::Init(init_2).clone()).await.unwrap();
    // Use a large enough timeout to ensure fin_receiver was canceled due to invalid init,
    // not due to a timeout.
    let fin_receiver = context
        .validate_proposal(proposal_init(HEIGHT_0, ROUND_0), TIMEOUT * 100, content_receiver)
        .await;
    assert_eq!(fin_receiver.await, Err(Canceled));
    // TODO(guyn): How to check that the rejection is due to the l1_gas_price_fri mismatch?
}

#[rstest]
#[case::maximum(true)]
#[case::minimum(false)]
#[tokio::test]
async fn gas_price_limits(#[case] maximum: bool) {
    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_deps_for_validate(SetupDepsArgs::default());
    let context_config = ContextDynamicConfig::default();
    let min_gas_price = context_config.min_l1_gas_price_wei;
    let min_data_price = context_config.min_l1_data_gas_price_wei;
    let max_gas_price = context_config.max_l1_gas_price_wei;
    let max_data_price = context_config.max_l1_data_gas_price_wei;

    let measured_price = if maximum {
        // Take the higher maximum price and go much higher than that.
        // If we don't go much higher, the l1_data_gas_price_multiplier will
        // lower the data gas price below the clamp limit.
        std::cmp::max(max_gas_price, max_data_price) * 100
    } else {
        0
    };
    let mut l1_gas_price_provider = MockL1GasPriceProviderClient::new();
    l1_gas_price_provider.expect_get_eth_to_fri_rate().returning(|_| Ok(ETH_TO_FRI_RATE));
    l1_gas_price_provider.expect_get_price_info().returning(move |_| {
        Ok(PriceInfo {
            base_fee_per_gas: GasPrice(measured_price),
            blob_fee: GasPrice(measured_price),
        })
    });

    deps.l1_gas_price_provider = l1_gas_price_provider;
    let mut context = deps.build_context();

    context.set_height_and_round(HEIGHT_0, ROUND_0).await.unwrap();

    let mut init = proposal_init(HEIGHT_0, ROUND_0);

    if maximum {
        // Set the gas price to the maximum value.
        init.l1_gas_price_wei = GasPrice(max_gas_price);
        init.l1_data_gas_price_wei = GasPrice(max_data_price);
        init.l1_gas_price_fri = init.l1_gas_price_wei.wei_to_fri(ETH_TO_FRI_RATE).unwrap();
        init.l1_data_gas_price_fri =
            init.l1_data_gas_price_wei.wei_to_fri(ETH_TO_FRI_RATE).unwrap();
    } else {
        // Set the gas price to the minimum value.
        init.l1_gas_price_wei = GasPrice(min_gas_price);
        init.l1_data_gas_price_wei = GasPrice(min_data_price);
        init.l1_gas_price_fri = init.l1_gas_price_wei.wei_to_fri(ETH_TO_FRI_RATE).unwrap();
        init.l1_data_gas_price_fri =
            init.l1_data_gas_price_wei.wei_to_fri(ETH_TO_FRI_RATE).unwrap();
    }

    // Send transactions and then fin.
    let content_receiver = send_proposal_to_validator_context(&mut context).await;

    // Even though we used the minimum/maximum gas price, not the values we gave the provider,
    // the proposal should be still be valid due to the clamping of limit prices.
    let fin_receiver = context.validate_proposal(init, TIMEOUT, content_receiver).await;
    assert_eq!(fin_receiver.await, Ok(TEST_PROPOSAL_COMMITMENT));
}

#[tokio::test]
async fn decision_reached_sends_correct_values() {
    let (mut deps, _network) = create_test_and_network_deps();

    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    // We need to create a valid proposal to call decision_reached on.
    //
    // 1. Build proposal setup starts.
    deps.setup_deps_for_build(SetupDepsArgs::default());

    const BLOCK_TIME_STAMP_SECONDS: u64 = 123456;
    let mut clock = MockClock::new();
    clock.expect_unix_now().return_const(BLOCK_TIME_STAMP_SECONDS);
    clock
        .expect_now()
        .return_const(Utc.timestamp_opt(BLOCK_TIME_STAMP_SECONDS.try_into().unwrap(), 0).unwrap());
    deps.clock = Arc::new(clock);

    // 2. Decision reached setup starts.
    deps.batcher
        .expect_decision_reached()
        .times(1)
        .return_once(move |_| Ok(DecisionReachedResponse::default()));

    // This is the actual part of the test that checks the values are correct.
    // TODO(guy.f): Add expectations and validations for all the other values being written.
    deps.state_sync_client.expect_add_new_block().times(1).return_once(|proposal_init| {
        assert_eq!(proposal_init.block_header_without_hash.timestamp.0, BLOCK_TIME_STAMP_SECONDS);
        Ok(())
    });

    deps.cende_ambassador
        .expect_prepare_blob_for_next_height()
        // TODO(guy.f): Verify the values sent here are correct.
        .return_once(|_height| Ok(()));

    let mut context = deps.build_context();

    // This sets up the required state for the test, prior to running the code being tested.
    let _fin = context.build_proposal(BuildParam::default(), TIMEOUT).await.unwrap().await;
    // At this point we should have a valid proposal in the context which contains the timestamp.

    context.decision_reached(HEIGHT_0, ROUND_0, TEST_PROPOSAL_COMMITMENT, false).await.unwrap();

    let metrics = recorder.handle().render();
    CONSENSUS_L2_GAS_PRICE
        .assert_eq(&metrics, VersionedConstants::latest_constants().min_gas_price.0);
}

/// Verify that when `stop_at_height` is set and decision is reached at that height:
/// 1. `wait_for_block_hash` retries until the batcher has computed the hash.
/// 2. The blob contains the block hash of that height.
/// 3. `write_prev_height_blob` is called immediately (not during next round proposal).
#[tokio::test]
async fn decision_reached_at_stop_height_writes_blob_immediately() {
    const STOP_HEIGHT: BlockNumber = BlockNumber(20);
    // The hash of block n is BlockHash(n), giving each block a distinct, predictable hash.
    let block_hash_fn = |n: u64| BlockHash(StarkHash::from(n));

    let (mut deps, _network) = create_test_and_network_deps();

    // Expectations added BEFORE setup_deps_for_validate so they are matched first (FIFO).

    // Heights 10–19: already committed; get_block_hash returns immediately for both the
    // retrospective_block_hash check (called during validate_proposal) and
    // collect_recent_block_hashes (called during finalize_decision).
    deps.batcher
        .expect_get_block_hash()
        .withf(|n| n.0 >= 10 && n.0 < STOP_HEIGHT.0)
        .returning(move |n| Ok(block_hash_fn(n.0)));

    // The retrospective_block_hash logic also queries state_sync for block 10.
    deps.state_sync_client
        .expect_get_block_hash()
        .withf(|n| n.0 == 10)
        .return_once(move |n| Ok(block_hash_fn(n.0)));

    // Height STOP_HEIGHT: first 2 calls return BlockHashNotFound (batcher not yet ready),
    // then all subsequent calls succeed — covering both wait_for_block_hash (3rd call)
    // and the collect_recent_block_hashes pass.
    deps.batcher
        .expect_get_block_hash()
        .withf(move |n| *n == STOP_HEIGHT)
        .times(2)
        .returning(|n| Err(BatcherClientError::BatcherError(BatcherError::BlockHashNotFound(n))));
    deps.batcher
        .expect_get_block_hash()
        .withf(move |n| *n == STOP_HEIGHT)
        .returning(move |n| Ok(block_hash_fn(n.0)));

    // write_prev_height_blob must be called immediately after decision, with height STOP_HEIGHT+1.
    deps.cende_ambassador
        .expect_write_prev_height_blob()
        .withf(move |h| *h == STOP_HEIGHT.unchecked_next())
        .times(1)
        .return_once(|_| tokio::spawn(ready(true)));

    deps.setup_deps_for_validate(SetupDepsArgs {
        start_block_number: STOP_HEIGHT,
        ..Default::default()
    });

    deps.batcher
        .expect_decision_reached()
        .times(1)
        .return_once(|_| Ok(DecisionReachedResponse::default()));

    deps.state_sync_client.expect_add_new_block().times(1).return_once(|_| Ok(()));

    // Verify the blob contains the block hash of the stop height.
    deps.cende_ambassador.expect_prepare_blob_for_next_height().times(1).return_once(
        move |params| {
            assert_eq!(
                params.recent_block_hashes.len(),
                usize::try_from(N_BLOCK_HASHES_BACK_IN_BLOB + 1).unwrap()
            );
            assert!(
                params.recent_block_hashes.last().is_some_and(
                    |bhn| bhn.number == STOP_HEIGHT && bhn.hash == block_hash_fn(STOP_HEIGHT.0)
                ),
                "Blob's recent_block_hashes should include block {STOP_HEIGHT}'s hash"
            );
            Ok(())
        },
    );

    let mut context = deps.build_context();
    // Make wait_for_block_hash retries instant so the test doesn't sleep 500ms per retry.
    context.config.static_config.retrospective_block_hash_retry_interval_millis =
        Duration::from_millis(10);

    context.set_height_and_round(STOP_HEIGHT, ROUND_0).await.unwrap();

    let content_receiver = send_proposal_to_validator_context(&mut context).await;
    let fin_receiver = context
        .validate_proposal(proposal_init(STOP_HEIGHT, ROUND_0), TIMEOUT, content_receiver)
        .await;
    let proposal_commitment = fin_receiver.await.unwrap();

    context.decision_reached(STOP_HEIGHT, ROUND_0, proposal_commitment, true).await.unwrap();
}

#[rstest]
#[case::l1_price_oracle_failure(true)]
#[case::eth_to_strk_rate_oracle_failure(false)]
#[tokio::test]
async fn oracle_fails_on_startup(#[case] l1_oracle_failure: bool) {
    let (mut deps, mut network) = create_test_and_network_deps();
    deps.setup_deps_for_build(SetupDepsArgs::default());

    if l1_oracle_failure {
        let mut l1_prices_oracle_client = MockL1GasPriceProviderClient::new();
        l1_prices_oracle_client.expect_get_eth_to_fri_rate().returning(|_| Ok(ETH_TO_FRI_RATE));
        l1_prices_oracle_client.expect_get_price_info().times(1).return_const(Err(
            L1GasPriceClientError::L1GasPriceProviderError(
                // random error, these parameters don't mean anything
                L1GasPriceProviderError::UnexpectedBlockNumberError { expected: 0, found: 1 },
            ),
        ));
        deps.l1_gas_price_provider = l1_prices_oracle_client;
    } else {
        let mut l1_prices_oracle_client = MockL1GasPriceProviderClient::new();
        l1_prices_oracle_client.expect_get_price_info().returning(|_| {
            Ok(PriceInfo {
                base_fee_per_gas: GasPrice(TEMP_ETH_GAS_FEE_IN_WEI),
                blob_fee: GasPrice(TEMP_ETH_BLOB_GAS_FEE_IN_WEI),
            })
        });
        l1_prices_oracle_client.expect_get_eth_to_fri_rate().times(1).return_once(|_| {
            Err(L1GasPriceClientError::PriceOracleClientError(
                PriceOracleClientError::MissingFieldError("".to_string(), "".to_string()),
            ))
        });
        deps.l1_gas_price_provider = l1_prices_oracle_client;
    }

    let mut context = deps.build_context();

    let build_param = BuildParam::default();

    let fin_receiver = context.build_proposal(build_param, TIMEOUT).await.unwrap();

    let (_, mut receiver) = network.outbound_proposal_receiver.next().await.unwrap();

    let part = receiver.next().await.unwrap();
    let ProposalPart::Init(info) = part else {
        panic!("Expected ProposalPart::Init");
    };

    let default_context_config = ContextDynamicConfig::default();
    // Despite the l1_gas_price_provider being set up not to fail, we still expect the default
    // values because eth_to_strk_rate_oracle_client failed.
    assert_eq!(info.l1_gas_price_wei.0, default_context_config.min_l1_gas_price_wei);
    assert_eq!(info.l1_data_gas_price_wei.0, default_context_config.min_l1_data_gas_price_wei);

    assert_eq!(
        receiver.next().await.unwrap(),
        ProposalPart::Transactions(TransactionBatch { transactions: TX_BATCH.to_vec() })
    );
    assert_eq!(
        receiver.next().await.unwrap(),
        ProposalPart::Fin(ProposalFin {
            proposal_commitment: TEST_PROPOSAL_COMMITMENT,
            executed_transaction_count: INTERNAL_TX_BATCH.len().try_into().unwrap(),
            fin_payload: Some(ProposalFinPayload {
                commitment_parts: CommitmentParts::default(),
                l2_gas_info: expected_l2_gas_info_for_build_proposal_defaults(),
            }),
        })
    );
    assert!(receiver.next().await.is_none());
    assert_eq!(fin_receiver.await.unwrap(), TEST_PROPOSAL_COMMITMENT);
}

#[rstest]
#[case::l1_price_oracle_failure(true)]
#[case::eth_to_strk_rate_oracle_failure(false)]
#[tokio::test]
async fn oracle_fails_on_second_block(#[case] l1_oracle_failure: bool) {
    let (mut deps, mut network) = create_test_and_network_deps();
    // Validate block number 0, call decision_reached to save the previous block info (block 0), and
    // attempt to build_proposal on block number 1.
    deps.setup_deps_for_validate(SetupDepsArgs::default());
    deps.setup_deps_for_build(SetupDepsArgs { start_block_number: HEIGHT_1, ..Default::default() });

    // set up batcher decision_reached
    deps.batcher.expect_decision_reached().times(1).return_once(|_| {
        Ok(DecisionReachedResponse {
            state_diff: ThinStateDiff::default(),
            central_objects: CentralObjects::default(),
        })
    });

    // required for decision reached flow
    deps.state_sync_client.expect_add_new_block().times(1).return_once(|_| Ok(()));
    deps.cende_ambassador.expect_prepare_blob_for_next_height().times(1).return_once(|_| Ok(()));

    // set the oracle to succeed on first block and fail on second
    if l1_oracle_failure {
        let mut l1_prices_oracle_client = MockL1GasPriceProviderClient::new();
        l1_prices_oracle_client.expect_get_eth_to_fri_rate().returning(|_| Ok(ETH_TO_FRI_RATE));
        l1_prices_oracle_client.expect_get_price_info().times(1).return_const(Ok(PriceInfo {
            base_fee_per_gas: GasPrice(TEMP_ETH_GAS_FEE_IN_WEI),
            blob_fee: GasPrice(TEMP_ETH_BLOB_GAS_FEE_IN_WEI),
        }));
        l1_prices_oracle_client.expect_get_price_info().times(1).return_const(Err(
            L1GasPriceClientError::L1GasPriceProviderError(
                // random error, these parameters don't mean anything
                L1GasPriceProviderError::UnexpectedBlockNumberError { expected: 0, found: 1 },
            ),
        ));
        deps.l1_gas_price_provider = l1_prices_oracle_client;
    } else {
        let mut l1_prices_oracle_client = MockL1GasPriceProviderClient::new();
        // Make sure the L1 gas price always returns with good values.
        l1_prices_oracle_client.expect_get_price_info().returning(|_| {
            Ok(PriceInfo {
                base_fee_per_gas: GasPrice(TEMP_ETH_GAS_FEE_IN_WEI),
                blob_fee: GasPrice(TEMP_ETH_BLOB_GAS_FEE_IN_WEI),
            })
        });
        // Set the eth_to_fri_rate to succeed on first block and fail on second.
        l1_prices_oracle_client
            .expect_get_eth_to_fri_rate()
            .times(1)
            .return_once(|_| Ok(ETH_TO_FRI_RATE));
        // Set the eth_to_fri_rate to fail on second block.
        l1_prices_oracle_client.expect_get_eth_to_fri_rate().times(1).return_once(|_| {
            Err(L1GasPriceClientError::PriceOracleClientError(
                PriceOracleClientError::MissingFieldError("".to_string(), "".to_string()),
            ))
        });
        deps.l1_gas_price_provider = l1_prices_oracle_client;
    }

    let mut context = deps.build_context();

    // Validate block number 0.

    // Initialize the context for a specific height, starting with round 0.
    context.set_height_and_round(HEIGHT_0, ROUND_0).await.unwrap();

    let content_receiver = send_proposal_to_validator_context(&mut context).await;
    let fin_receiver = context
        .validate_proposal(proposal_init(HEIGHT_0, ROUND_0), TIMEOUT, content_receiver)
        .await;
    let proposal_commitment = fin_receiver.await.unwrap();
    assert_eq!(proposal_commitment, TEST_PROPOSAL_COMMITMENT);

    // Decision reached

    context.decision_reached(HEIGHT_0, ROUND_0, proposal_commitment, false).await.unwrap();

    // Build proposal for block number 1.
    let build_param = BuildParam { height: HEIGHT_1, ..Default::default() };

    let fin_receiver = context.build_proposal(build_param, TIMEOUT).await.unwrap();

    let (_, mut receiver) = network.outbound_proposal_receiver.next().await.unwrap();

    let part = receiver.next().await.unwrap();
    let ProposalPart::Init(info) = part else {
        panic!("Expected ProposalPart::Init");
    };
    assert_eq!(info.height, HEIGHT_1);

    let previous_init = proposal_init(HEIGHT_0, ROUND_0);

    assert_eq!(info.l1_gas_price_wei, previous_init.l1_gas_price_wei);
    assert_eq!(info.l1_data_gas_price_wei, previous_init.l1_data_gas_price_wei);
    assert_eq!(info.l1_gas_price_fri, previous_init.l1_gas_price_fri);
    assert_eq!(info.l1_data_gas_price_fri, previous_init.l1_data_gas_price_fri);

    assert_eq!(
        receiver.next().await.unwrap(),
        ProposalPart::Transactions(TransactionBatch { transactions: TX_BATCH.to_vec() })
    );
    assert_eq!(
        receiver.next().await.unwrap(),
        ProposalPart::Fin(ProposalFin {
            proposal_commitment: TEST_PROPOSAL_COMMITMENT,
            executed_transaction_count: INTERNAL_TX_BATCH.len().try_into().unwrap(),
            fin_payload: Some(ProposalFinPayload {
                commitment_parts: CommitmentParts::default(),
                l2_gas_info: expected_l2_gas_info_for_build_proposal_defaults(),
            }),
        })
    );
    assert!(receiver.next().await.is_none());
    assert_eq!(fin_receiver.await.unwrap(), TEST_PROPOSAL_COMMITMENT);
}

// L2 gas is a bit above the minimum gas price.
const ODDLY_SPECIFIC_L2_GAS_PRICE: u128 = 9999999999;
const ODDLY_SPECIFIC_L1_GAS_PRICE: u128 = 1234567890;
const ODDLY_SPECIFIC_L1_DATA_GAS_PRICE: u128 = 987654321;
const ODDLY_SPECIFIC_CONVERSION_RATE: u128 = 12345678901234567890;

// If we use low numbers for fri/wei we have to make sure the conversion (eth to fri) and eth-to-wei
// factor (wei to eth) don't go below zero in either direction. Typically the gas price (in
// particular the data gas price) can be as low as 1, so the eth-to-fri rate must be above 10^18.
// That also means that the L2 gas (in fri) must be bigger than the ratio of the conversion rate and
// the eth-to-wei factor. Must use a large enough number that conversion to wei works
const LOW_OVERRIDE_L2_GAS_PRICE: u128 = 25; // FRI
// Must be larger than 10 since ETH_TO_WEI is 10^18 and LOW_OVERRIDE_CONVERSION_RATE is 10^19
const LOW_OVERRIDE_L1_GAS_PRICE: u128 = 100; // FRI
const LOW_OVERRIDE_L1_DATA_GAS_PRICE: u128 = 100; // FRI
// ETH_TO_FRI_RATE must be larger/equal to 10^18 (wei to eth conversion factor)
const LOW_OVERRIDE_CONVERSION_RATE: u128 = u128::pow(10, 19);

// If we use really low L2 gas price, the block will fail to build.
const LOW_OVERRIDE_L2_GAS_PRICE_FAIL: u128 = 1; // FRI

#[rstest]
#[case::dont_override_prices(None, None, None, None, true)]
#[case::override_l2_gas_price(Some(ODDLY_SPECIFIC_L2_GAS_PRICE), None, None, None, true)]
#[case::override_l1_gas_price(None, Some(ODDLY_SPECIFIC_L1_GAS_PRICE), None, None, true)]
#[case::override_l1_data_gas_price(None, None, Some(ODDLY_SPECIFIC_L1_DATA_GAS_PRICE), None, true)]
#[case::override_eth_to_strk_rate(None, None, None, Some(ODDLY_SPECIFIC_CONVERSION_RATE), true)]
#[case::override_all_prices(
    Some(ODDLY_SPECIFIC_L2_GAS_PRICE),
    Some(ODDLY_SPECIFIC_L1_GAS_PRICE),
    Some(ODDLY_SPECIFIC_L1_DATA_GAS_PRICE),
    None,
    true
)]
#[case::override_everything(
    Some(ODDLY_SPECIFIC_L2_GAS_PRICE),
    Some(ODDLY_SPECIFIC_L1_GAS_PRICE),
    Some(ODDLY_SPECIFIC_L1_DATA_GAS_PRICE),
    Some(ODDLY_SPECIFIC_CONVERSION_RATE),
    true
)]
#[case::low_overrides(
    Some(LOW_OVERRIDE_L2_GAS_PRICE),
    Some(LOW_OVERRIDE_L1_GAS_PRICE),
    Some(LOW_OVERRIDE_L1_DATA_GAS_PRICE),
    Some(LOW_OVERRIDE_CONVERSION_RATE),
    true
)]
#[case::low_l2_gas_price_fail(
    Some(LOW_OVERRIDE_L2_GAS_PRICE_FAIL),
    None,
    None,
    Some(LOW_OVERRIDE_CONVERSION_RATE),
    false
)]
#[tokio::test]
async fn override_prices_behavior(
    #[case] override_l2_gas_price_fri: Option<u128>,
    #[case] override_l1_gas_price_fri: Option<u128>,
    #[case] override_l1_data_gas_price_fri: Option<u128>,
    #[case] override_eth_to_fri_rate: Option<u128>,
    #[case] build_success: bool,
) {
    // Use high gas usage to ensure the L2 gas price is high.
    let high_l2_gas_used = VersionedConstants::latest_constants().max_block_size;

    let (mut deps, _network) = create_test_and_network_deps();

    // Setup dependencies and mocks.
    #[allow(clippy::as_conversions)]
    deps.setup_deps_for_build(SetupDepsArgs {
        number_of_times: build_success as usize,
        l2_gas_used: Some(high_l2_gas_used),
        ..Default::default()
    });
    if !build_success {
        // We use number_of_times equal zero in this case, but we still expect the start height to
        // be called.
        deps.batcher.expect_start_height().times(1).return_once(|_| Ok(()));
    }
    deps.batcher.expect_decision_reached().return_once(move |_| {
        Ok(DecisionReachedResponse {
            state_diff: ThinStateDiff::default(),
            central_objects: CentralObjects::default(),
        })
    });

    deps.state_sync_client.expect_add_new_block().return_once(|_| Ok(()));
    deps.cende_ambassador.expect_prepare_blob_for_next_height().return_once(|_| Ok(()));

    let context_config = ContextConfig {
        dynamic_config: ContextDynamicConfig {
            override_l2_gas_price_fri,
            override_l1_gas_price_fri,
            override_l1_data_gas_price_fri,
            override_eth_to_fri_rate,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut context = deps.build_context();
    context.config = context_config;

    let min_gas_price = VersionedConstants::latest_constants().min_gas_price.0;
    let gas_price_params = make_gas_price_params(&context.config.dynamic_config);
    let mut expected_l1_prices = PriceInfo {
        base_fee_per_gas: GasPrice(TEMP_ETH_GAS_FEE_IN_WEI),
        blob_fee: GasPrice(TEMP_ETH_BLOB_GAS_FEE_IN_WEI),
    };
    apply_fee_transformations(&mut expected_l1_prices, &gas_price_params);

    // Run proposal and decision logic.
    let fin_result = context.build_proposal(BuildParam::default(), TIMEOUT).await.unwrap().await;

    // In cases where we expect the batcher to fail the block build.
    if build_success {
        assert!(fin_result.is_ok(), "Expected build to succeed, but got error: {:?}", fin_result);
    } else {
        // The build fails because the L2 gas price in wei we get, after using the eth/fri rate we
        // calculated from the block info, is zero.
        assert!(fin_result.is_err(), "Expected build to fail, but got success: {:?}", fin_result);
        return;
    }

    context.decision_reached(HEIGHT_0, ROUND_0, TEST_PROPOSAL_COMMITMENT, false).await.unwrap();

    let actual_l2_gas_price = context.l2_gas_price.0;

    let previous_block = context.previous_proposal_init.clone().unwrap();
    let actual_l1_gas_price = previous_block.l1_prices_fri.l1_gas_price.0;
    let actual_l1_data_gas_price = previous_block.l1_prices_fri.l1_data_gas_price.0;
    let actual_conversion_rate = previous_block
        .l1_prices_fri
        .l1_gas_price
        .0
        .checked_mul(WEI_PER_ETH)
        .unwrap()
        .checked_div(previous_block.l1_prices_wei.l1_gas_price.0)
        .unwrap();

    let expected_wei_to_fri_rate = override_eth_to_fri_rate.unwrap_or(ETH_TO_FRI_RATE);
    let expected_l1_gas_price_fri = GasPrice(expected_l1_prices.base_fee_per_gas.0)
        .wei_to_fri(expected_wei_to_fri_rate)
        .unwrap()
        .0;
    let expected_l1_data_gas_price_fri =
        GasPrice(expected_l1_prices.blob_fee.0).wei_to_fri(expected_wei_to_fri_rate).unwrap().0;

    if let Some(override_l2_gas_price) = override_l2_gas_price_fri {
        // In this case the L2 gas price must match the given override.
        assert_eq!(
            actual_l2_gas_price, override_l2_gas_price,
            "Mismatch in L2 gas price. Actual: {actual_l2_gas_price} expected (override) l2 gas \
             price: {override_l2_gas_price}.",
        );
    } else {
        // In this case the regular L2 gas calculation takes place, and gives a higher price.
        assert!(
            actual_l2_gas_price > min_gas_price,
            "Mismatch in L2 gas price. Actual: {actual_l2_gas_price} expected (minimum) l2 gas \
             price: {min_gas_price} due to high usage (EIP-1559).",
        );
    }

    if let Some(override_l1_gas_price) = override_l1_gas_price_fri {
        assert_eq!(
            actual_l1_gas_price, override_l1_gas_price,
            "Mismatch in L1 gas price. Actual: {actual_l1_gas_price} expected (override) l1 gas \
             price: {override_l1_gas_price}.",
        );
    } else {
        assert_eq!(
            actual_l1_gas_price, expected_l1_gas_price_fri,
            "Mismatch in L1 gas price. Actual: {actual_l1_gas_price} expected l1 gas price: \
             {expected_l1_gas_price_fri} (after conversion from wei to FRI).",
        );
    }

    if let Some(override_l1_data_gas_price) = override_l1_data_gas_price_fri {
        assert_eq!(
            actual_l1_data_gas_price, override_l1_data_gas_price,
            "Mismatch in L1 data gas price. Actual: {actual_l1_data_gas_price} expected \
             (override) l1 data gas price: {override_l1_data_gas_price}.",
        );
    } else {
        assert_eq!(
            actual_l1_data_gas_price, expected_l1_data_gas_price_fri,
            "Mismatch in L1 data gas price. Actual: {actual_l1_data_gas_price} expected l1 data \
             gas price: {expected_l1_data_gas_price_fri} (after conversion from wei to FRI).",
        );
    }

    // Conversion rate is recreated by comparing wei and fri prices, so it is affected by rounding
    // errors.
    if let Some(override_eth_to_fri_rate) = override_eth_to_fri_rate {
        assert!(
            almost_equal(actual_conversion_rate, override_eth_to_fri_rate),
            "Mismatch in conversion rate. Actual: {actual_conversion_rate}. expected conversion \
             rate: {override_eth_to_fri_rate}",
        );
    }
}

/// Check that two numbers are within 0.1% of each other.
fn almost_equal(a: u128, b: u128) -> bool {
    a.abs_diff(b) < a / 1000
}

#[tokio::test]
async fn change_gas_price_overrides() {
    let (mut deps, mut network) = create_test_and_network_deps();

    // Validate two blocks, between the first and the second we will change the gas price overrides.
    // After the second block we do another round with another dynamic config change.
    // Finally, we start a new round as proposer, with a third dynamic config change before it
    // starts.
    deps.setup_deps_for_validate(SetupDepsArgs { number_of_times: 2, ..Default::default() });
    deps.setup_deps_for_validate(SetupDepsArgs {
        number_of_times: 1,
        expect_start_height: false,
        start_block_number: BlockNumber(2),
        ..Default::default()
    });
    deps.setup_deps_for_build(SetupDepsArgs {
        start_block_number: BlockNumber(2),
        ..Default::default()
    });

    deps.batcher.expect_decision_reached().times(2).returning(|_| {
        Ok(DecisionReachedResponse {
            state_diff: ThinStateDiff::default(),
            central_objects: CentralObjects::default(),
        })
    });

    // required for decision reached flow
    deps.state_sync_client.expect_add_new_block().times(2).returning(|_| Ok(()));
    deps.cende_ambassador.expect_prepare_blob_for_next_height().times(2).returning(|_| Ok(()));

    let mut context = deps.build_context();

    // Validate block number 0.
    context.set_height_and_round(HEIGHT_0, ROUND_0).await.unwrap();

    let content_receiver = send_proposal_to_validator_context(&mut context).await;
    let fin_receiver = context
        .validate_proposal(proposal_init(HEIGHT_0, ROUND_0), TIMEOUT, content_receiver)
        .await;

    let proposal_commitment = fin_receiver.await.unwrap();
    assert_eq!(proposal_commitment, TEST_PROPOSAL_COMMITMENT);

    context.decision_reached(HEIGHT_0, ROUND_0, proposal_commitment, false).await.unwrap();

    let new_dynamic_config = ContextDynamicConfig {
        override_l2_gas_price_fri: Some(ODDLY_SPECIFIC_L2_GAS_PRICE),
        ..Default::default()
    };
    let config_manager_client = make_config_manager_client(new_dynamic_config);
    context.deps.config_manager_client = Some(Arc::new(config_manager_client));

    // Validate block number 1, round 0.
    context.set_height_and_round(HEIGHT_1, ROUND_0).await.unwrap();

    // This should fail, since the gas price is different from the input block info.
    let content_receiver = send_proposal_to_validator_context(&mut context).await;
    let fin_receiver = context
        .validate_proposal(proposal_init(HEIGHT_1, ROUND_0), TIMEOUT, content_receiver)
        .await;
    let proposal_commitment = fin_receiver.await.unwrap_err();
    assert!(matches!(proposal_commitment, Canceled));

    // Modify the incoming init to make sure it matches the overrides. Now it passes.
    let mut modified_init = proposal_init(HEIGHT_1, ROUND_0);
    modified_init.l2_gas_price_fri = GasPrice(ODDLY_SPECIFIC_L2_GAS_PRICE);

    let content_receiver = send_proposal_to_validator_context(&mut context).await;
    let fin_receiver = context.validate_proposal(modified_init, TIMEOUT, content_receiver).await;
    let proposal_commitment = fin_receiver.await.unwrap();
    assert_eq!(proposal_commitment, TEST_PROPOSAL_COMMITMENT);

    // Validate block number 1, round 1.
    let new_dynamic_config = ContextDynamicConfig {
        override_l1_data_gas_price_fri: Some(ODDLY_SPECIFIC_L1_DATA_GAS_PRICE),
        ..Default::default()
    };
    let config_manager_client = make_config_manager_client(new_dynamic_config);
    context.deps.config_manager_client = Some(Arc::new(config_manager_client));

    // This should fail, as we have changed the config, without updating the block info.
    context.set_height_and_round(HEIGHT_1, ROUND_1).await.unwrap();

    let content_receiver = send_proposal_to_validator_context(&mut context).await;
    let fin_receiver = context
        .validate_proposal(proposal_init(HEIGHT_1, ROUND_1), TIMEOUT, content_receiver)
        .await;
    let proposal_commitment = fin_receiver.await.unwrap_err();
    assert!(matches!(proposal_commitment, Canceled));

    // Add the new overrides so validation passes.
    let mut modified_init = proposal_init(HEIGHT_1, ROUND_1);
    modified_init.l1_data_gas_price_fri = GasPrice(ODDLY_SPECIFIC_L1_DATA_GAS_PRICE);
    // Note that the eth to fri conversion rate by default is 10^18 so we can just replace wei to
    // fri 1:1.
    modified_init.l1_data_gas_price_fri = GasPrice(ODDLY_SPECIFIC_L1_DATA_GAS_PRICE);

    let content_receiver = send_proposal_to_validator_context(&mut context).await;
    let fin_receiver = context.validate_proposal(modified_init, TIMEOUT, content_receiver).await;
    let proposal_commitment = fin_receiver.await.unwrap();
    assert_eq!(proposal_commitment, TEST_PROPOSAL_COMMITMENT);

    context.decision_reached(HEIGHT_1, ROUND_1, proposal_commitment, false).await.unwrap();

    // Now build a proposal for height 2.
    let new_dynamic_config = ContextDynamicConfig {
        override_eth_to_fri_rate: Some(ODDLY_SPECIFIC_CONVERSION_RATE),
        ..Default::default()
    };
    let config_manager_client = make_config_manager_client(new_dynamic_config);
    context.deps.config_manager_client = Some(Arc::new(config_manager_client));

    let fin_receiver = context
        .build_proposal(BuildParam { height: BlockNumber(2), ..Default::default() }, TIMEOUT)
        .await
        .unwrap()
        .await
        .unwrap();

    assert_eq!(fin_receiver, TEST_PROPOSAL_COMMITMENT);
    let (_, mut receiver) = network.outbound_proposal_receiver.next().await.unwrap();

    let part = receiver.next().await.unwrap();
    let ProposalPart::Init(_) = part else {
        panic!("Expected ProposalPart::Init");
    };
}

fn make_config_manager_client(provider_config: ContextDynamicConfig) -> MockConfigManagerClient {
    let mut config_manager_client = MockConfigManagerClient::new();
    config_manager_client
        .expect_get_context_dynamic_config()
        .returning(move || Ok(provider_config.clone()));
    config_manager_client.expect_set_node_dynamic_config().returning(|_| Ok(()));
    config_manager_client
}

// Flow: dynamic config changes the min gas price per height while the node is running.
// Config updates do not immediately overwrite current gas price; they take effect on the
// next block gas-price calculation, which updates gradually.
#[tokio::test]
async fn test_dynamic_config_updates_min_gas_price() {
    // Test constants
    const INITIAL_CONFIG_HEIGHT: u64 = 100;
    const INITIAL_CONFIG_MIN_PRICE: u128 = 10_000_000_000;
    const NEW_CONFIG_HEIGHT: u64 = 200;
    const NEW_CONFIG_MIN_PRICE: u128 = 20_000_000_000;

    const INITIAL_HEIGHT: u64 = 150; // Node is already running at this height
    const TEST_HEIGHT: u64 = 250; // Move to this height with updated config
    const CURRENT_GAS_PRICE: u128 = 15_000_000_000; // Below new minimum

    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_default_expectations();

    // Create a mock config manager client that will return dynamic config
    let mut mock_config_manager = MockConfigManagerClient::new();

    // Mock expects get_context_dynamic_config to be called once (when moving to new height)
    // This is called inside set_height_and_round() -> update_dynamic_config() ->
    // client.get_context_dynamic_config()

    // Config returns updated min price at NEW_CONFIG_HEIGHT
    mock_config_manager.expect_get_context_dynamic_config().times(1).returning(move || {
        Ok(ContextDynamicConfig {
            min_l2_gas_price_per_height: vec![
                PricePerHeight { height: INITIAL_CONFIG_HEIGHT, price: INITIAL_CONFIG_MIN_PRICE },
                PricePerHeight { height: NEW_CONFIG_HEIGHT, price: NEW_CONFIG_MIN_PRICE },
            ],
            ..Default::default()
        })
    });

    // Setup batcher expectation for one height
    // set_height_and_round() calls batcher.start_height() to notify the batcher
    deps.batcher.expect_start_height().times(1).returning(|_| Ok(()));

    // Convert TestDeps to SequencerConsensusContextDeps and add config manager
    let mut context_deps: SequencerConsensusContextDeps = deps.into();
    context_deps.config_manager_client = Some(Arc::new(mock_config_manager));

    let mut context = SequencerConsensusContext::new(
        ContextConfig {
            static_config: ContextStaticConfig {
                proposal_buffer_size: CHANNEL_SIZE,
                chain_id: CHAIN_ID,
                ..Default::default()
            },
            dynamic_config: ContextDynamicConfig { ..Default::default() },
        },
        context_deps,
    );

    // Simulate node already running: set current_height to make context "already initialized"
    context.current_height = Some(BlockNumber(INITIAL_HEIGHT));
    context.l2_gas_price = GasPrice(INITIAL_CONFIG_MIN_PRICE);

    // Move to TEST_HEIGHT with updated config
    context.set_height_and_round(BlockNumber(TEST_HEIGHT), ROUND_0).await.unwrap();

    // Verify dynamic config was updated
    assert_eq!(context.config.dynamic_config.min_l2_gas_price_per_height.len(), 2);
    assert_eq!(
        context.config.dynamic_config.min_l2_gas_price_per_height[1].price,
        NEW_CONFIG_MIN_PRICE
    );

    // Set gas price below new minimum to simulate being below the updated config
    context.l2_gas_price = GasPrice(CURRENT_GAS_PRICE);

    // Simulate gas price update with new config in effect
    context.update_l2_gas_price(BlockNumber(TEST_HEIGHT), GasAmount(1000));

    // Gas price should have increased gradually towards new minimum
    // Formula: new_price = min(price + price/333, min_gas_price)
    // Starting at CURRENT_GAS_PRICE (15 Gwei)
    // max_increase = 15_000_000_000 / 333 = 45_045_045
    // expected = 15_000_000_000 + 45_045_045 = 15_045_045_045
    const MIN_GAS_PRICE_INCREASE_DENOMINATOR: u128 = 333;
    let expected_price =
        CURRENT_GAS_PRICE + (CURRENT_GAS_PRICE / MIN_GAS_PRICE_INCREASE_DENOMINATOR);
    let expected_price = expected_price.min(NEW_CONFIG_MIN_PRICE);

    let actual_price = context.l2_gas_price.0;
    assert_eq!(
        actual_price, expected_price,
        "Gas price should be exactly {} (15 Gwei + 15/333), got {}",
        expected_price, actual_price
    );
}

// Flow: node starts processing without syncing first (e.g., after revert or sync unavailable).
// Gas price begins at versioned-constants fallback, then bootstrap enforces the configured
// minimum for the current height.
#[tokio::test]
async fn test_first_height_uses_configured_min_l2_gas_price_for_height() {
    const CONFIG_HEIGHT_1: u64 = 100;
    const CONFIG_PRICE_1: u128 = 10_000_000_000;
    const CONFIG_HEIGHT_2: u64 = 200;
    const CONFIG_PRICE_2: u128 = 20_000_000_000;
    const STARTUP_HEIGHT: u64 = 250;
    const LOW_STARTUP_PRICE: u128 = 8_000_000_000;

    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_default_expectations();
    deps.batcher.expect_start_height().times(1).returning(|_| Ok(()));

    let mut context = deps.build_context();
    context.l2_gas_price = GasPrice(LOW_STARTUP_PRICE);
    context.config.dynamic_config.min_l2_gas_price_per_height = vec![
        PricePerHeight { height: CONFIG_HEIGHT_1, price: CONFIG_PRICE_1 },
        PricePerHeight { height: CONFIG_HEIGHT_2, price: CONFIG_PRICE_2 },
    ];

    // First set_height_and_round call on this new context instance.
    context.set_height_and_round(BlockNumber(STARTUP_HEIGHT), ROUND_0).await.unwrap();

    // Height 250 is after config height 200, so gas price should initialize to CONFIG_PRICE_2.
    assert_eq!(context.l2_gas_price, GasPrice(CONFIG_PRICE_2));
}

// Flow: sync provides next_l2_gas_price at height 200, but config defines a higher
// minimum starting at height 250. At height 200, the synced value is kept.
// Later at height 250, config minimum is applied gradually.
#[tokio::test]
async fn test_first_height_keeps_sync_provided_l2_gas_price() {
    const SYNC_HEIGHT: BlockNumber = BlockNumber(200);
    const LATER_HEIGHT: BlockNumber = BlockNumber(250);
    const SYNCED_NEXT_L2_GAS_PRICE: u128 = 20_000_000_000;
    const CONFIG_MIN_PRICE_AT_250: u128 = 25_000_000_000;

    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_default_expectations();
    deps.batcher.expect_add_sync_block().times(1).return_once(|_| Ok(()));
    deps.batcher.expect_start_height().times(2).returning(|_| Ok(()));
    deps.state_sync_client.expect_get_block().times(1).return_once(|height| {
        let mut sync_block = SyncBlock::default();
        sync_block.block_header_without_hash.block_number = height;
        sync_block.block_header_without_hash.next_l2_gas_price = GasPrice(SYNCED_NEXT_L2_GAS_PRICE);
        Ok(sync_block)
    });

    let mut context = deps.build_context();
    context.config.dynamic_config.min_l2_gas_price_per_height =
        vec![PricePerHeight { height: 250, price: CONFIG_MIN_PRICE_AT_250 }];

    // Sync succeeds at height 200, l2_gas_price is taken from synced next_l2_gas_price.
    assert!(context.try_sync(SYNC_HEIGHT).await);
    assert_eq!(context.l2_gas_price, GasPrice(SYNCED_NEXT_L2_GAS_PRICE));

    // First height initialization at 200: synced value is kept.
    context.set_height_and_round(SYNC_HEIGHT, ROUND_0).await.unwrap();
    assert_eq!(context.l2_gas_price, GasPrice(SYNCED_NEXT_L2_GAS_PRICE));

    // Move to height 250 where config min applies
    context.set_height_and_round(LATER_HEIGHT, ROUND_0).await.unwrap();
    // Bootstrap doesn't run (not first height anymore), price still 20g
    assert_eq!(context.l2_gas_price, GasPrice(SYNCED_NEXT_L2_GAS_PRICE));

    // Subsequent block should gradually increase toward CONFIG_MIN_PRICE_AT_250
    context.update_l2_gas_price(LATER_HEIGHT, GasAmount(1000));

    const MIN_GAS_PRICE_INCREASE_DENOMINATOR: u128 = 333;
    let expected_price =
        SYNCED_NEXT_L2_GAS_PRICE + (SYNCED_NEXT_L2_GAS_PRICE / MIN_GAS_PRICE_INCREASE_DENOMINATOR);
    assert_eq!(
        context.l2_gas_price.0, expected_price,
        "Gas price should be {} (20 Gwei + 20/333), got {}",
        expected_price, context.l2_gas_price.0
    );
}
