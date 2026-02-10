use std::future::ready;
use std::sync::Arc;

use apollo_batcher_types::batcher_types::{
    CentralObjects,
    DecisionReachedResponse,
    ProposalCommitment as BatcherProposalCommitment,
    ProposalStatus,
    SendProposalContent,
    SendProposalContentResponse,
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
use apollo_infra::component_client::ClientError;
use apollo_l1_gas_price_types::errors::{
    EthToStrkOracleClientError,
    L1GasPriceClientError,
    L1GasPriceProviderError,
};
use apollo_l1_gas_price_types::{MockL1GasPriceProviderClient, PriceInfo};
use apollo_protobuf::consensus::{
    BuildParam,
    ProposalCommitment,
    ProposalFin,
    ProposalPart,
    TransactionBatch,
};
use apollo_state_sync_types::communication::{MockStateSyncClient, StateSyncClientError};
use apollo_time::time::MockClock;
use chrono::{TimeZone, Utc};
use futures::channel::mpsc;
use futures::channel::oneshot::Canceled;
use futures::future::pending;
use futures::{FutureExt, SinkExt, StreamExt};
use metrics_exporter_prometheus::PrometheusBuilder;
use rstest::rstest;
use starknet_api::block::{
    BlockNumber,
    GasPrice,
    TEMP_ETH_BLOB_GAS_FEE_IN_WEI,
    TEMP_ETH_GAS_FEE_IN_WEI,
    WEI_PER_ETH,
};
use starknet_api::block_hash::block_hash_calculator::BlockHeaderCommitments;
use starknet_api::execution_resources::GasAmount;
use starknet_api::state::ThinStateDiff;
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;

use crate::cende::MockCendeContext;
use crate::metrics::CONSENSUS_L2_GAS_PRICE;
use crate::orchestrator_versioned_constants::VersionedConstants;
use crate::sequencer_consensus_context::{
    SequencerConsensusContext,
    SequencerConsensusContextDeps,
};
use crate::test_utils::{
    block_info,
    create_test_and_network_deps,
    send_proposal_to_validator_context,
    SetupDepsArgs,
    CHAIN_ID,
    CHANNEL_SIZE,
    ETH_TO_FRI_RATE,
    INTERNAL_TX_BATCH,
    STATE_DIFF_COMMITMENT,
    TIMEOUT,
    TX_BATCH,
};
use crate::utils::{apply_fee_transformations, make_gas_price_params};

#[tokio::test]
async fn cancelled_proposal_aborts() {
    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_default_expectations();

    deps.batcher.expect_propose_block().times(1).return_const(Ok(()));
    deps.batcher.expect_start_height().times(1).return_const(Ok(()));

    let mut context = deps.build_context();
    let fin_receiver = context.build_proposal(BuildParam::default(), TIMEOUT).await.unwrap();

    // Now we intrrupt the proposal and verify that the fin_receiever is dropped.
    context.set_height_and_round(BlockNumber(0), 1).await.unwrap();

    assert_eq!(fin_receiver.await, Err(Canceled));
}

#[tokio::test]
async fn validate_proposal_success() {
    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_deps_for_validate(SetupDepsArgs::default());
    let mut context = deps.build_context();

    // Initialize the context for a specific height, starting with round 0.
    context.set_height_and_round(BlockNumber(0), 0).await.unwrap();
    let content_receiver = send_proposal_to_validator_context(&mut context).await;
    let fin_receiver =
        context.validate_proposal(block_info(BlockNumber(0), 0), TIMEOUT, content_receiver).await;
    assert_eq!(fin_receiver.await.unwrap().0, STATE_DIFF_COMMITMENT.0.0);
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
    let mut context = deps.build_context();

    // Initialize the context for a specific height, starting with round 0.
    context.set_height_and_round(BlockNumber(0), 0).await.unwrap();

    // Receive a valid proposal.
    let (mut content_sender, content_receiver) =
        mpsc::channel(context.config.static_config.proposal_buffer_size);
    let init = block_info(BlockNumber(0), 0);
    let transactions =
        ProposalPart::Transactions(TransactionBatch { transactions: TX_BATCH.to_vec() });
    content_sender.send(transactions.clone()).await.unwrap();
    let fin = ProposalPart::Fin(ProposalFin {
        proposal_commitment: ProposalCommitment(STATE_DIFF_COMMITMENT.0.0),
        executed_transaction_count: n_executed_txs_count.try_into().unwrap(),
        commitment_parts: None,
    });
    content_sender.send(fin.clone()).await.unwrap();
    let fin_receiver = context.validate_proposal(init.clone(), TIMEOUT, content_receiver).await;
    content_sender.close_channel();
    assert_eq!(fin_receiver.await.unwrap().0, STATE_DIFF_COMMITMENT.0.0);

    let build_param = BuildParam { round: 1, ..Default::default() };
    context.repropose(ProposalCommitment(STATE_DIFF_COMMITMENT.0.0), build_param).await;
    let (_, mut receiver) = network.outbound_proposal_receiver.next().await.unwrap();
    assert_eq!(receiver.next().await.unwrap(), ProposalPart::Init(init));
    assert_eq!(
        receiver.next().await.unwrap(),
        ProposalPart::Transactions(TransactionBatch { transactions: executed_transactions })
    );
    assert_eq!(receiver.next().await.unwrap(), fin);
    assert!(receiver.next().await.is_none());
}

#[tokio::test]
async fn proposals_from_different_rounds() {
    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_deps_for_validate(SetupDepsArgs::default());
    let mut context = deps.build_context();
    // Initialize the context for a specific height, starting with round 0.
    context.set_height_and_round(BlockNumber(0), 0).await.unwrap();
    context.set_height_and_round(BlockNumber(0), 1).await.unwrap();

    // Proposal parts sent in the proposals.
    let prop_part_txs =
        ProposalPart::Transactions(TransactionBatch { transactions: TX_BATCH.to_vec() });
    let prop_part_fin = ProposalPart::Fin(ProposalFin {
        proposal_commitment: ProposalCommitment(STATE_DIFF_COMMITMENT.0.0),
        executed_transaction_count: INTERNAL_TX_BATCH.len().try_into().unwrap(),
        commitment_parts: None,
    });

    // The proposal from the past round is ignored.
    let (mut content_sender, content_receiver) =
        mpsc::channel(context.config.static_config.proposal_buffer_size);
    content_sender.send(prop_part_txs.clone()).await.unwrap();

    let fin_receiver_past_round =
        context.validate_proposal(block_info(BlockNumber(0), 0), TIMEOUT, content_receiver).await;
    // No fin was sent, channel remains open.
    assert!(fin_receiver_past_round.await.is_err());

    // The proposal from the current round should be validated.
    let (mut content_sender, content_receiver) =
        mpsc::channel(context.config.static_config.proposal_buffer_size);
    content_sender.send(prop_part_txs.clone()).await.unwrap();
    content_sender.send(prop_part_fin.clone()).await.unwrap();
    let fin_receiver_curr_round =
        context.validate_proposal(block_info(BlockNumber(0), 1), TIMEOUT, content_receiver).await;
    assert_eq!(fin_receiver_curr_round.await.unwrap().0, STATE_DIFF_COMMITMENT.0.0);

    // The proposal from the future round should not be processed.
    let (mut content_sender, content_receiver) =
        mpsc::channel(context.config.static_config.proposal_buffer_size);
    content_sender.send(prop_part_txs.clone()).await.unwrap();
    content_sender.send(prop_part_fin.clone()).await.unwrap();
    let fin_receiver_future_round =
        context.validate_proposal(block_info(BlockNumber(0), 2), TIMEOUT, content_receiver).await;
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
    deps.batcher
        .expect_start_height()
        .withf(|input| input.height == BlockNumber(0))
        .return_const(Ok(()));

    // Round 0: Will be interrupted and send Abort
    deps.batcher.expect_send_proposal_content().times(1).returning(|input| {
        assert!(matches!(input.content, SendProposalContent::Abort));
        Ok(SendProposalContentResponse { response: ProposalStatus::Processing })
    });

    // Round 1: Will send Txs then Finish
    deps.batcher.expect_send_proposal_content().times(1).returning(|input| {
        let SendProposalContent::Txs(txs) = input.content else {
            panic!("Expected Txs");
        };
        assert_eq!(txs, *INTERNAL_TX_BATCH);
        Ok(SendProposalContentResponse { response: ProposalStatus::Processing })
    });
    deps.batcher.expect_send_proposal_content().times(1).returning(|input| {
        assert!(matches!(input.content, SendProposalContent::Finish(_)));
        Ok(SendProposalContentResponse {
            response: ProposalStatus::Finished(BatcherProposalCommitment {
                state_diff_commitment: STATE_DIFF_COMMITMENT,
            }),
        })
    });

    let mut context = deps.build_context();
    // Initialize the context for a specific height, starting with round 0.
    context.set_height_and_round(BlockNumber(0), 0).await.unwrap();

    // Keep the sender open, as closing it or sending Fin would cause the validate to complete
    // without needing interrupt.
    let (mut _content_sender_0, content_receiver) =
        mpsc::channel(context.config.static_config.proposal_buffer_size);
    let fin_receiver_0 =
        context.validate_proposal(block_info(BlockNumber(0), 0), TIMEOUT, content_receiver).await;

    let content_receiver = send_proposal_to_validator_context(&mut context).await;
    let fin_receiver_1 =
        context.validate_proposal(block_info(BlockNumber(0), 1), TIMEOUT, content_receiver).await;
    // Move the context to the next round.
    context.set_height_and_round(BlockNumber(0), 1).await.unwrap();

    // Interrupt active proposal.
    assert!(fin_receiver_0.await.is_err());
    assert_eq!(fin_receiver_1.await.unwrap().0, STATE_DIFF_COMMITMENT.0.0);
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
            proposal_commitment: ProposalCommitment(STATE_DIFF_COMMITMENT.0.0),
            executed_transaction_count: INTERNAL_TX_BATCH.len().try_into().unwrap(),
            commitment_parts: None,
        })
    );
    assert!(receiver.next().await.is_none());
    assert_eq!(fin_receiver.await.unwrap().0, STATE_DIFF_COMMITMENT.0.0);
}

#[tokio::test]
async fn build_proposal_skips_write_for_height_0() {
    let (mut deps, _network) = create_test_and_network_deps();

    deps.batcher.expect_start_height().returning(|_| Ok(()));

    // Make sure the state sync client isn't called by clearing its expectations.
    deps.state_sync_client = MockStateSyncClient::new();

    // Clear the "default" expectations on the cende ambassador. If we can skip writing the blob,
    // should not be called at all.
    deps.cende_ambassador = MockCendeContext::new();

    let mut context = deps.build_context();
    let _fin_receiver = context
        .build_proposal(BuildParam { height: BlockNumber(0), ..Default::default() }, TIMEOUT)
        .await;
}

#[tokio::test]
async fn build_proposal_skips_write_for_height_above_0() {
    // Important: We set the height to be under 10 to avoid triggering the code path which writes
    // the block hash mapping for height - 10 (which we're not testing as part of this test)
    const HEIGHT: BlockNumber = BlockNumber(9);

    let (mut deps, _network) = create_test_and_network_deps();

    deps.setup_deps_for_build(SetupDepsArgs { start_block_number: HEIGHT, ..Default::default() });

    // We already have the previous block in sync:
    deps.state_sync_client
        .expect_get_latest_block_number()
        .returning(|| Ok(Some(HEIGHT.prev().unwrap())));

    // Clear the "default" expectations on the cende ambassador. If we can skip writing the blob,
    // should not be called at all.
    deps.cende_ambassador = MockCendeContext::new();

    let mut context = deps.build_context();
    let _fin_receiver =
        context.build_proposal(BuildParam { height: HEIGHT, ..Default::default() }, TIMEOUT).await;
}

#[tokio::test]
async fn build_proposal_writes_prev_blob_if_cannot_get_latest_block_number() {
    // We set a non zero height to make sure a call to get_latest_block_number is made.
    //
    // Important: We set the height to be under 10 to avoid triggering the code path which writes
    // the block hash mapping for height - 10 (which we're not testing as part of this test)
    const HEIGHT: BlockNumber = BlockNumber(9);

    let (mut deps, _network) = create_test_and_network_deps();

    deps.setup_deps_for_build(SetupDepsArgs { start_block_number: HEIGHT, ..Default::default() });

    deps.state_sync_client.expect_get_latest_block_number().returning(|| {
        Err(StateSyncClientError::ClientError(ClientError::CommunicationFailure("".to_string())))
    });

    let mut mock_cende_context = MockCendeContext::new();
    mock_cende_context
        .expect_write_prev_height_blob()
        .times(1)
        .return_once(|_| tokio::spawn(ready(true)));
    deps.cende_ambassador = mock_cende_context;

    let mut context = deps.build_context();
    let fin_receiver = context
        .build_proposal(BuildParam { height: HEIGHT, ..Default::default() }, TIMEOUT)
        .await
        .unwrap();

    assert_eq!(fin_receiver.await, Ok(ProposalCommitment(STATE_DIFF_COMMITMENT.0.0)));
}

#[tokio::test]
async fn build_proposal_cende_failure() {
    // We write a height that isn't 0 since for height 0 we skip writing the blob (and we want to
    // test a write failure).
    //
    // Important: We set the height to be under 10 to avoid triggering the code path which writes
    // the block hash mapping for height - 10 (which we're not testing as part of this test)
    const HEIGHT: BlockNumber = BlockNumber(9);

    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_deps_for_build(SetupDepsArgs { start_block_number: HEIGHT, ..Default::default() });
    // We do not have the previous block in sync, so we must try to write the previous height blob.
    deps.state_sync_client
        .expect_get_latest_block_number()
        .returning(|| Ok(Some(HEIGHT.prev().unwrap().prev().unwrap())));

    let mut mock_cende_context = MockCendeContext::new();
    mock_cende_context
        .expect_write_prev_height_blob()
        .times(1)
        .return_once(|_height| tokio::spawn(ready(false)));
    deps.cende_ambassador = mock_cende_context;
    let mut context = deps.build_context();

    let fin_receiver = context
        .build_proposal(BuildParam { height: HEIGHT, ..Default::default() }, TIMEOUT)
        .await
        .unwrap();
    assert_eq!(fin_receiver.await, Err(Canceled));
}

#[tokio::test]
async fn build_proposal_cende_incomplete() {
    // We write a height that isn't 0 since for height 0 we skip writing the blob (and we want to
    // test a write failure).
    //
    // Important: We set the height to be under 10 to avoid triggering the code path which writes
    // the block hash mapping for height - 10 (which we're not testing as part of this test)
    const HEIGHT: BlockNumber = BlockNumber(9);

    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_deps_for_build(SetupDepsArgs { start_block_number: HEIGHT, ..Default::default() });
    // We do not have the previous block in sync, so we must try to write the previous height blob.
    deps.state_sync_client
        .expect_get_latest_block_number()
        .returning(|| Ok(Some(HEIGHT.prev().unwrap().prev().unwrap())));

    let mut mock_cende_context = MockCendeContext::new();
    mock_cende_context
        .expect_write_prev_height_blob()
        .times(1)
        .return_once(|_height| tokio::spawn(pending()));
    deps.cende_ambassador = mock_cende_context;
    let mut context = deps.build_context();

    let fin_receiver = context
        .build_proposal(BuildParam { height: HEIGHT, ..Default::default() }, TIMEOUT)
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
    context.set_height_and_round(BlockNumber::default(), Round::default()).await.unwrap();

    if proposer {
        let fin_receiver = context.build_proposal(BuildParam::default(), TIMEOUT).await.unwrap();
        assert_eq!(fin_receiver.await, Err(Canceled));
    } else {
        let (_content_sender, content_receiver) =
            mpsc::channel(context.config.static_config.proposal_buffer_size);

        let fin_receiver = context
            .validate_proposal(block_info(BlockNumber(0), 0), TIMEOUT, content_receiver)
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
    let mut context = deps.build_context();
    // Build proposal.
    let fin_receiver = context.build_proposal(BuildParam::default(), TIMEOUT).await.unwrap();
    let (_, mut receiver) = network.outbound_proposal_receiver.next().await.unwrap();
    // Receive the proposal parts.
    let part = receiver.next().await.unwrap();
    let _txs = receiver.next().await.unwrap();
    let fin = receiver.next().await.unwrap();
    assert_eq!(fin_receiver.await.unwrap().0, STATE_DIFF_COMMITMENT.0.0);

    // Re-propose.
    context
        .repropose(
            ProposalCommitment(STATE_DIFF_COMMITMENT.0.0),
            BuildParam { round: 1, ..Default::default() },
        )
        .await;
    // Re-propose sends the same proposal.
    let (_, mut receiver) = network.outbound_proposal_receiver.next().await.unwrap();
    assert_eq!(receiver.next().await.unwrap(), part);

    let reproposed_txs = ProposalPart::Transactions(TransactionBatch { transactions });
    assert_eq!(receiver.next().await.unwrap(), reproposed_txs);

    assert_eq!(receiver.next().await.unwrap(), fin);
    assert!(receiver.next().await.is_none());
}

#[tokio::test]
async fn gas_price_fri_out_of_range() {
    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_default_expectations();

    deps.batcher
        .expect_start_height()
        .times(1)
        .withf(|input| input.height == BlockNumber(0))
        .return_const(Ok(()));
    let mut context = deps.build_context();
    context.set_height_and_round(BlockNumber(0), 0).await.unwrap();
    let (_content_sender, content_receiver) =
        mpsc::channel(context.config.static_config.proposal_buffer_size);
    // Receive a block info with l1_gas_price_fri that is outside the margin of error.
    let mut init_1 = block_info(BlockNumber(0), 0);
    init_1.l1_gas_price_fri = init_1.l1_gas_price_fri.checked_mul_u128(2).unwrap();
    // Use a large enough timeout to ensure fin_receiver was canceled due to invalid init,
    // not due to a timeout.
    let fin_receiver = context.validate_proposal(init_1, TIMEOUT * 100, content_receiver).await;
    assert_eq!(fin_receiver.await, Err(Canceled));

    // Do the same for data gas price.
    let (mut content_sender, content_receiver) =
        mpsc::channel(context.config.static_config.proposal_buffer_size);
    let mut init_2 = block_info(BlockNumber(0), 0);
    init_2.l1_data_gas_price_fri = init_2.l1_data_gas_price_fri.checked_mul_u128(2).unwrap();
    content_sender.send(ProposalPart::Init(init_2).clone()).await.unwrap();
    // Use a large enough timeout to ensure fin_receiver was canceled due to invalid init,
    // not due to a timeout.
    let fin_receiver = context
        .validate_proposal(block_info(BlockNumber(0), 0), TIMEOUT * 100, content_receiver)
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

    context.set_height_and_round(BlockNumber(0), 0).await.unwrap();

    let mut init = block_info(BlockNumber(0), 0);

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
    assert_eq!(fin_receiver.await, Ok(ProposalCommitment(STATE_DIFF_COMMITMENT.0.0)));
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
    deps.state_sync_client.expect_add_new_block().times(1).return_once(|block_info| {
        assert_eq!(block_info.block_header_without_hash.timestamp.0, BLOCK_TIME_STAMP_SECONDS);
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

    context
        .decision_reached(BlockNumber(0), ProposalCommitment(STATE_DIFF_COMMITMENT.0.0))
        .await
        .unwrap();

    let metrics = recorder.handle().render();
    CONSENSUS_L2_GAS_PRICE
        .assert_eq(&metrics, VersionedConstants::latest_constants().min_gas_price.0);
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
            Err(L1GasPriceClientError::EthToStrkOracleClientError(
                EthToStrkOracleClientError::MissingFieldError("".to_string(), "".to_string()),
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
            proposal_commitment: ProposalCommitment(STATE_DIFF_COMMITMENT.0.0),
            executed_transaction_count: INTERNAL_TX_BATCH.len().try_into().unwrap(),
            commitment_parts: None,
        })
    );
    assert!(receiver.next().await.is_none());
    assert_eq!(fin_receiver.await.unwrap().0, STATE_DIFF_COMMITMENT.0.0);
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
    deps.setup_deps_for_build(SetupDepsArgs {
        start_block_number: BlockNumber(1),
        ..Default::default()
    });

    // set up batcher decision_reached
    deps.batcher.expect_decision_reached().times(1).return_once(|_| {
        Ok(DecisionReachedResponse {
            state_diff: ThinStateDiff::default(),
            l2_gas_used: GasAmount::default(),
            central_objects: CentralObjects::default(),
            block_header_commitments: BlockHeaderCommitments::default(),
        })
    });

    // required for decision reached flow
    deps.state_sync_client.expect_add_new_block().times(1).return_once(|_| Ok(()));
    // We never wrote block 0.
    deps.state_sync_client.expect_get_latest_block_number().returning(|| Ok(None));
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
            Err(L1GasPriceClientError::EthToStrkOracleClientError(
                EthToStrkOracleClientError::MissingFieldError("".to_string(), "".to_string()),
            ))
        });
        deps.l1_gas_price_provider = l1_prices_oracle_client;
    }

    let mut context = deps.build_context();

    // Validate block number 0.

    // Initialize the context for a specific height, starting with round 0.
    context.set_height_and_round(BlockNumber(0), 0).await.unwrap();

    let content_receiver = send_proposal_to_validator_context(&mut context).await;
    let fin_receiver =
        context.validate_proposal(block_info(BlockNumber(0), 0), TIMEOUT, content_receiver).await;
    let proposal_commitment = fin_receiver.await.unwrap();
    assert_eq!(proposal_commitment.0, STATE_DIFF_COMMITMENT.0.0);

    // Decision reached

    context.decision_reached(BlockNumber(0), proposal_commitment).await.unwrap();

    // Build proposal for block number 1.
    let build_param = BuildParam { height: BlockNumber(1), ..Default::default() };

    let fin_receiver = context.build_proposal(build_param, TIMEOUT).await.unwrap();

    let (_, mut receiver) = network.outbound_proposal_receiver.next().await.unwrap();

    let part = receiver.next().await.unwrap();
    let ProposalPart::Init(info) = part else {
        panic!("Expected ProposalPart::Init");
    };
    assert_eq!(info.height, BlockNumber(1));

    let previous_init = block_info(BlockNumber(0), 0);

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
            proposal_commitment: ProposalCommitment(STATE_DIFF_COMMITMENT.0.0),
            executed_transaction_count: INTERNAL_TX_BATCH.len().try_into().unwrap(),
            commitment_parts: None,
        })
    );
    assert!(receiver.next().await.is_none());
    assert_eq!(fin_receiver.await.unwrap().0, STATE_DIFF_COMMITMENT.0.0);
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
    let mock_l2_gas_used = VersionedConstants::latest_constants().max_block_size;

    let (mut deps, _network) = create_test_and_network_deps();

    // Setup dependencies and mocks.
    #[allow(clippy::as_conversions)]
    deps.setup_deps_for_build(SetupDepsArgs {
        number_of_times: build_success as usize,
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
            l2_gas_used: mock_l2_gas_used,
            central_objects: CentralObjects::default(),
            block_header_commitments: BlockHeaderCommitments::default(),
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

    context
        .decision_reached(BlockNumber(0), ProposalCommitment(STATE_DIFF_COMMITMENT.0.0))
        .await
        .unwrap();

    let actual_l2_gas_price = context.l2_gas_price.0;

    let previous_block = context.previous_block_info.clone().unwrap();
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
            l2_gas_used: GasAmount::default(),
            central_objects: CentralObjects::default(),
            block_header_commitments: BlockHeaderCommitments::default(),
        })
    });

    // required for decision reached flow
    deps.state_sync_client.expect_add_new_block().times(2).returning(|_| Ok(()));
    // Mock sync to never provide any blocks in this test.
    deps.state_sync_client.expect_get_latest_block_number().returning(|| Ok(None));
    deps.cende_ambassador.expect_prepare_blob_for_next_height().times(2).returning(|_| Ok(()));

    let mut context = deps.build_context();

    // Validate block number 0.
    context.set_height_and_round(BlockNumber(0), 0).await.unwrap();

    let content_receiver = send_proposal_to_validator_context(&mut context).await;
    let fin_receiver =
        context.validate_proposal(block_info(BlockNumber(0), 0), TIMEOUT, content_receiver).await;

    let proposal_commitment = fin_receiver.await.unwrap();
    assert_eq!(proposal_commitment.0, STATE_DIFF_COMMITMENT.0.0);

    context.decision_reached(BlockNumber(0), proposal_commitment).await.unwrap();

    let new_dynamic_config = ContextDynamicConfig {
        override_l2_gas_price_fri: Some(ODDLY_SPECIFIC_L2_GAS_PRICE),
        ..Default::default()
    };
    let config_manager_client = make_config_manager_client(new_dynamic_config);
    context.deps.config_manager_client = Some(Arc::new(config_manager_client));

    // Validate block number 1, round 0.
    context.set_height_and_round(BlockNumber(1), 0).await.unwrap();

    // This should fail, since the gas price is different from the input block info.
    let content_receiver = send_proposal_to_validator_context(&mut context).await;
    let fin_receiver =
        context.validate_proposal(block_info(BlockNumber(1), 0), TIMEOUT, content_receiver).await;
    let proposal_commitment = fin_receiver.await.unwrap_err();
    assert!(matches!(proposal_commitment, Canceled));

    // Modify the incoming init to make sure it matches the overrides. Now it passes.
    let mut modified_init = block_info(BlockNumber(1), 0);
    modified_init.l2_gas_price_fri = GasPrice(ODDLY_SPECIFIC_L2_GAS_PRICE);

    let content_receiver = send_proposal_to_validator_context(&mut context).await;
    let fin_receiver = context.validate_proposal(modified_init, TIMEOUT, content_receiver).await;
    let proposal_commitment = fin_receiver.await.unwrap();
    assert_eq!(proposal_commitment.0, STATE_DIFF_COMMITMENT.0.0);

    // Validate block number 1, round 1.
    let new_dynamic_config = ContextDynamicConfig {
        override_l1_data_gas_price_fri: Some(ODDLY_SPECIFIC_L1_DATA_GAS_PRICE),
        ..Default::default()
    };
    let config_manager_client = make_config_manager_client(new_dynamic_config);
    context.deps.config_manager_client = Some(Arc::new(config_manager_client));

    // This should fail, as we have changed the config, without updating the block info.
    context.set_height_and_round(BlockNumber(1), 1).await.unwrap();

    let content_receiver = send_proposal_to_validator_context(&mut context).await;
    let fin_receiver =
        context.validate_proposal(block_info(BlockNumber(1), 1), TIMEOUT, content_receiver).await;
    let proposal_commitment = fin_receiver.await.unwrap_err();
    assert!(matches!(proposal_commitment, Canceled));

    // Add the new overrides so validation passes.
    let mut modified_init = block_info(BlockNumber(1), 1);
    modified_init.l1_data_gas_price_fri = GasPrice(ODDLY_SPECIFIC_L1_DATA_GAS_PRICE);
    // Note that the eth to fri conversion rate by default is 10^18 so we can just replace wei to
    // fri 1:1.
    modified_init.l1_data_gas_price_fri = GasPrice(ODDLY_SPECIFIC_L1_DATA_GAS_PRICE);

    let content_receiver = send_proposal_to_validator_context(&mut context).await;
    let fin_receiver = context.validate_proposal(modified_init, TIMEOUT, content_receiver).await;
    let proposal_commitment = fin_receiver.await.unwrap();
    assert_eq!(proposal_commitment.0, STATE_DIFF_COMMITMENT.0.0);

    context.decision_reached(BlockNumber(1), proposal_commitment).await.unwrap();

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

    assert_eq!(fin_receiver.0, STATE_DIFF_COMMITMENT.0.0);
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

#[tokio::test]
async fn test_dynamic_config_updates_min_gas_price() {
    // Test constants
    const FIRST_CONFIG_HEIGHT: u64 = 100;
    const FIRST_CONFIG_MIN_PRICE: u128 = 10_000_000_000;
    const SECOND_CONFIG_HEIGHT: u64 = 200;
    const SECOND_CONFIG_MIN_PRICE: u128 = 20_000_000_000;

    const INITIAL_GAS_PRICE: u128 = 8_000_000_000; // below first minimum
    const FIRST_TEST_HEIGHT: u64 = 150; // Between 100 and 200
    const SECOND_TEST_HEIGHT: u64 = 250; // Above 200
    const INTERMEDIATE_GAS_PRICE: u128 = 15_000_000_000; // Below second minimum

    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_default_expectations();

    // Create a mock config manager client that will return dynamic config
    let mut mock_config_manager = MockConfigManagerClient::new();

    // Mock expects get_context_dynamic_config to be called twice (once per height change)
    // This is called inside set_height_and_round() -> update_dynamic_config() ->
    // client.get_context_dynamic_config()

    // First call returns config with min price at FIRST_CONFIG_HEIGHT
    mock_config_manager.expect_get_context_dynamic_config().times(1).returning(move || {
        Ok(ContextDynamicConfig {
            min_l2_gas_price_per_height: vec![PricePerHeight {
                height: FIRST_CONFIG_HEIGHT,
                price: FIRST_CONFIG_MIN_PRICE,
            }],
            ..Default::default()
        })
    });

    // Second call returns config with additional min price at SECOND_CONFIG_HEIGHT
    mock_config_manager.expect_get_context_dynamic_config().times(1).returning(move || {
        Ok(ContextDynamicConfig {
            min_l2_gas_price_per_height: vec![
                PricePerHeight { height: FIRST_CONFIG_HEIGHT, price: FIRST_CONFIG_MIN_PRICE },
                PricePerHeight { height: SECOND_CONFIG_HEIGHT, price: SECOND_CONFIG_MIN_PRICE },
            ],
            ..Default::default()
        })
    });

    // Setup batcher expectations for two heights
    // set_height_and_round() calls batcher.start_height() to notify the batcher
    deps.batcher.expect_start_height().times(2).returning(|_| Ok(()));

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

    // Set initial L2 gas price below minimum
    context.l2_gas_price = GasPrice(INITIAL_GAS_PRICE);

    // Test at FIRST_TEST_HEIGHT: Should use min price from first config.
    // This calls set_height_and_round which triggers update_dynamic_config() internally
    context.set_height_and_round(BlockNumber(FIRST_TEST_HEIGHT), 0).await.unwrap();

    // Verify dynamic config was updated
    assert_eq!(context.config.dynamic_config.min_l2_gas_price_per_height.len(), 1);
    assert_eq!(
        context.config.dynamic_config.min_l2_gas_price_per_height[0].price,
        FIRST_CONFIG_MIN_PRICE
    );

    // Simulate gas price update - this calls the fee market logic with min_l2_gas_price_per_height
    context.update_l2_gas_price(BlockNumber(FIRST_TEST_HEIGHT), GasAmount(1000));

    // Gas price should have increased towards minimum (gradual adjustment)
    // Formula: new_price = min(price + price/333, min_gas_price)
    // Starting at INITIAL_GAS_PRICE (8 Gwei), with MIN_GAS_PRICE_INCREASE_DENOMINATOR = 333
    // max_increase = 8_000_000_000 / 333 = 24_024_024
    // expected = 8_000_000_000 + 24_024_024 = 8_024_024_024
    const MIN_GAS_PRICE_INCREASE_DENOMINATOR: u128 = 333;
    let expected_price_after_first =
        INITIAL_GAS_PRICE + (INITIAL_GAS_PRICE / MIN_GAS_PRICE_INCREASE_DENOMINATOR);
    let expected_price_after_first = expected_price_after_first.min(FIRST_CONFIG_MIN_PRICE);

    let price_after_first_update = context.l2_gas_price.0;
    assert_eq!(
        price_after_first_update, expected_price_after_first,
        "Gas price should be exactly {} (8 Gwei + 8/333), got {}",
        expected_price_after_first, price_after_first_update
    );

    // Test at SECOND_TEST_HEIGHT: Should use min price from config2
    context.set_height_and_round(BlockNumber(SECOND_TEST_HEIGHT), 0).await.unwrap();

    // Verify dynamic config was updated again
    assert_eq!(context.config.dynamic_config.min_l2_gas_price_per_height.len(), 2);
    assert_eq!(
        context.config.dynamic_config.min_l2_gas_price_per_height[1].price,
        SECOND_CONFIG_MIN_PRICE
    );

    // Set gas price below new minimum
    context.l2_gas_price = GasPrice(INTERMEDIATE_GAS_PRICE);

    // Simulate gas price update
    context.update_l2_gas_price(BlockNumber(SECOND_TEST_HEIGHT), GasAmount(1000));

    // Gas price should have increased towards new minimum
    // Formula: new_price = min(price + price/333, min_gas_price)
    // Starting at INTERMEDIATE_GAS_PRICE (15 Gwei)
    // max_increase = 15_000_000_000 / 333 = 45_045_045
    // expected = 15_000_000_000 + 45_045_045 = 15_045_045_045
    let expected_price_after_second =
        INTERMEDIATE_GAS_PRICE + (INTERMEDIATE_GAS_PRICE / MIN_GAS_PRICE_INCREASE_DENOMINATOR);
    let expected_price_after_second = expected_price_after_second.min(SECOND_CONFIG_MIN_PRICE);

    let price_after_second_update = context.l2_gas_price.0;
    assert_eq!(
        price_after_second_update, expected_price_after_second,
        "Gas price should be exactly {} (15 Gwei + 15/333), got {}",
        expected_price_after_second, price_after_second_update
    );
}
