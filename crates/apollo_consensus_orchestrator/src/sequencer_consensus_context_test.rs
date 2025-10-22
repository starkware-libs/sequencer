use std::future::ready;
use std::sync::Arc;
use std::vec;

use apollo_batcher_types::batcher_types::{CentralObjects, DecisionReachedResponse};
use apollo_batcher_types::communication::BatcherClientError;
use apollo_batcher_types::errors::BatcherError;
use apollo_consensus::types::{ConsensusContext, Round};
use apollo_l1_gas_price_types::errors::{
    EthToStrkOracleClientError,
    L1GasPriceClientError,
    L1GasPriceProviderError,
};
use apollo_l1_gas_price_types::{MockL1GasPriceProviderClient, PriceInfo, DEFAULT_ETH_TO_FRI_RATE};
use apollo_protobuf::consensus::{ProposalFin, ProposalInit, ProposalPart, TransactionBatch, Vote};
use apollo_time::time::MockClock;
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
};
use starknet_api::execution_resources::GasAmount;
use starknet_api::state::ThinStateDiff;

use crate::cende::MockCendeContext;
use crate::config::ContextConfig;
use crate::metrics::CONSENSUS_L2_GAS_PRICE;
use crate::orchestrator_versioned_constants::VersionedConstants;
use crate::test_utils::{
    block_info,
    create_test_and_network_deps,
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
    let fin_receiver = context.build_proposal(ProposalInit::default(), TIMEOUT).await;

    // Now we intrrupt the proposal and verify that the fin_receiever is dropped.
    context.set_height_and_round(BlockNumber(0), 1).await;

    assert_eq!(fin_receiver.await, Err(Canceled));
}

#[tokio::test]
async fn validate_proposal_success() {
    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_deps_for_validate(BlockNumber(0), INTERNAL_TX_BATCH.len(), 1);
    let mut context = deps.build_context();

    // Initialize the context for a specific height, starting with round 0.
    context.set_height_and_round(BlockNumber(0), 0).await;

    let (mut content_sender, content_receiver) = mpsc::channel(context.config.proposal_buffer_size);
    content_sender.send(ProposalPart::BlockInfo(block_info(BlockNumber(0)))).await.unwrap();
    content_sender
        .send(ProposalPart::Transactions(TransactionBatch { transactions: TX_BATCH.to_vec() }))
        .await
        .unwrap();
    content_sender
        .send(ProposalPart::ExecutedTransactionCount(INTERNAL_TX_BATCH.len().try_into().unwrap()))
        .await
        .unwrap();
    content_sender
        .send(ProposalPart::Fin(ProposalFin {
            proposal_commitment: BlockHash(STATE_DIFF_COMMITMENT.0.0),
        }))
        .await
        .unwrap();
    let fin_receiver =
        context.validate_proposal(ProposalInit::default(), TIMEOUT, content_receiver).await;
    content_sender.close_channel();
    assert_eq!(fin_receiver.await.unwrap().0, STATE_DIFF_COMMITMENT.0.0);
}

#[tokio::test]
async fn dont_send_block_info() {
    let (mut deps, _network) = create_test_and_network_deps();

    deps.batcher
        .expect_start_height()
        .times(1)
        .withf(|input| input.height == BlockNumber(0))
        .return_const(Ok(()));
    let mut context = deps.build_context();

    // Initialize the context for a specific height, starting with round 0.
    context.set_height_and_round(BlockNumber(0), 0).await;

    let (mut content_sender, content_receiver) = mpsc::channel(context.config.proposal_buffer_size);
    let fin_receiver =
        context.validate_proposal(ProposalInit::default(), TIMEOUT, content_receiver).await;
    content_sender.close_channel();
    // No block info was sent, the proposal is invalid.
    assert!(fin_receiver.await.is_err());
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
    let final_n_executed_txs = executed_transactions.len();
    deps.setup_deps_for_validate(BlockNumber(0), final_n_executed_txs, 1);
    let mut context = deps.build_context();

    // Initialize the context for a specific height, starting with round 0.
    context.set_height_and_round(BlockNumber(0), 0).await;

    // Receive a valid proposal.
    let (mut content_sender, content_receiver) = mpsc::channel(context.config.proposal_buffer_size);
    let block_info = ProposalPart::BlockInfo(block_info(BlockNumber(0)));
    content_sender.send(block_info.clone()).await.unwrap();
    let transactions =
        ProposalPart::Transactions(TransactionBatch { transactions: TX_BATCH.to_vec() });
    content_sender.send(transactions.clone()).await.unwrap();
    content_sender
        .send(ProposalPart::ExecutedTransactionCount(final_n_executed_txs.try_into().unwrap()))
        .await
        .unwrap();
    let fin = ProposalPart::Fin(ProposalFin {
        proposal_commitment: BlockHash(STATE_DIFF_COMMITMENT.0.0),
    });
    content_sender.send(fin.clone()).await.unwrap();
    let fin_receiver =
        context.validate_proposal(ProposalInit::default(), TIMEOUT, content_receiver).await;
    content_sender.close_channel();
    assert_eq!(fin_receiver.await.unwrap().0, STATE_DIFF_COMMITMENT.0.0);

    let init = ProposalInit { round: 1, ..Default::default() };
    context.repropose(BlockHash(STATE_DIFF_COMMITMENT.0.0), init).await;
    let (_, mut receiver) = network.outbound_proposal_receiver.next().await.unwrap();
    assert_eq!(receiver.next().await.unwrap(), ProposalPart::Init(init));
    assert_eq!(receiver.next().await.unwrap(), block_info);
    assert_eq!(
        receiver.next().await.unwrap(),
        ProposalPart::Transactions(TransactionBatch { transactions: executed_transactions })
    );
    assert_eq!(
        receiver.next().await.unwrap(),
        ProposalPart::ExecutedTransactionCount(final_n_executed_txs.try_into().unwrap())
    );
    assert_eq!(receiver.next().await.unwrap(), fin);
    assert!(receiver.next().await.is_none());
}

#[tokio::test]
async fn proposals_from_different_rounds() {
    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_deps_for_validate(BlockNumber(0), INTERNAL_TX_BATCH.len(), 1);
    let mut context = deps.build_context();
    // Initialize the context for a specific height, starting with round 0.
    context.set_height_and_round(BlockNumber(0), 0).await;
    context.set_height_and_round(BlockNumber(0), 1).await;

    // Proposal parts sent in the proposals.
    let prop_part_txs =
        ProposalPart::Transactions(TransactionBatch { transactions: TX_BATCH.to_vec() });
    let prop_part_executed_count =
        ProposalPart::ExecutedTransactionCount(INTERNAL_TX_BATCH.len().try_into().unwrap());
    let prop_part_fin = ProposalPart::Fin(ProposalFin {
        proposal_commitment: BlockHash(STATE_DIFF_COMMITMENT.0.0),
    });

    // The proposal from the past round is ignored.
    let (mut content_sender, content_receiver) = mpsc::channel(context.config.proposal_buffer_size);
    content_sender.send(ProposalPart::BlockInfo(block_info(BlockNumber(0)))).await.unwrap();
    content_sender.send(prop_part_txs.clone()).await.unwrap();
    content_sender.send(prop_part_executed_count.clone()).await.unwrap();

    let mut init = ProposalInit { round: 0, ..Default::default() };
    let fin_receiver_past_round = context.validate_proposal(init, TIMEOUT, content_receiver).await;
    // No fin was sent, channel remains open.
    assert!(fin_receiver_past_round.await.is_err());

    // The proposal from the current round should be validated.
    let (mut content_sender, content_receiver) = mpsc::channel(context.config.proposal_buffer_size);
    content_sender.send(ProposalPart::BlockInfo(block_info(BlockNumber(0)))).await.unwrap();
    content_sender.send(prop_part_txs.clone()).await.unwrap();
    content_sender.send(prop_part_executed_count.clone()).await.unwrap();
    content_sender.send(prop_part_fin.clone()).await.unwrap();
    init.round = 1;
    let fin_receiver_curr_round = context.validate_proposal(init, TIMEOUT, content_receiver).await;
    assert_eq!(fin_receiver_curr_round.await.unwrap().0, STATE_DIFF_COMMITMENT.0.0);

    // The proposal from the future round should not be processed.
    let (mut content_sender, content_receiver) = mpsc::channel(context.config.proposal_buffer_size);
    content_sender.send(ProposalPart::BlockInfo(block_info(BlockNumber(0)))).await.unwrap();
    content_sender.send(prop_part_txs.clone()).await.unwrap();
    content_sender.send(prop_part_executed_count.clone()).await.unwrap();
    content_sender.send(prop_part_fin.clone()).await.unwrap();
    let fin_receiver_future_round = context
        .validate_proposal(
            ProposalInit { round: 2, ..Default::default() },
            TIMEOUT,
            content_receiver,
        )
        .await;
    content_sender.close_channel();
    // Even with sending fin and closing the channel.
    assert!(fin_receiver_future_round.now_or_never().is_none());
}

#[tokio::test]
async fn interrupt_active_proposal() {
    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_deps_for_validate(BlockNumber(0), INTERNAL_TX_BATCH.len(), 1);
    let mut context = deps.build_context();
    // Initialize the context for a specific height, starting with round 0.
    context.set_height_and_round(BlockNumber(0), 0).await;

    // Keep the sender open, as closing it or sending Fin would cause the validate to complete
    // without needing interrupt.
    let (mut _content_sender_0, content_receiver) =
        mpsc::channel(context.config.proposal_buffer_size);
    let fin_receiver_0 =
        context.validate_proposal(ProposalInit::default(), TIMEOUT, content_receiver).await;

    let (mut content_sender_1, content_receiver) =
        mpsc::channel(context.config.proposal_buffer_size);
    content_sender_1.send(ProposalPart::BlockInfo(block_info(BlockNumber(0)))).await.unwrap();
    content_sender_1
        .send(ProposalPart::Transactions(TransactionBatch { transactions: TX_BATCH.to_vec() }))
        .await
        .unwrap();
    content_sender_1
        .send(ProposalPart::ExecutedTransactionCount(INTERNAL_TX_BATCH.len().try_into().unwrap()))
        .await
        .unwrap();
    content_sender_1
        .send(ProposalPart::Fin(ProposalFin {
            proposal_commitment: BlockHash(STATE_DIFF_COMMITMENT.0.0),
        }))
        .await
        .unwrap();
    let fin_receiver_1 = context
        .validate_proposal(
            ProposalInit { round: 1, ..Default::default() },
            TIMEOUT,
            content_receiver,
        )
        .await;
    // Move the context to the next round.
    context.set_height_and_round(BlockNumber(0), 1).await;

    // Interrupt active proposal.
    assert!(fin_receiver_0.await.is_err());
    assert_eq!(fin_receiver_1.await.unwrap().0, STATE_DIFF_COMMITMENT.0.0);
}

#[tokio::test]
async fn build_proposal() {
    let before: u64 =
        chrono::Utc::now().timestamp().try_into().expect("Timestamp conversion failed");
    let (mut deps, mut network) = create_test_and_network_deps();
    deps.setup_deps_for_build(BlockNumber(0), INTERNAL_TX_BATCH.len(), 1);
    let mut context = deps.build_context();
    let fin_receiver = context.build_proposal(ProposalInit::default(), TIMEOUT).await;
    // Test proposal parts.
    let (_, mut receiver) = network.outbound_proposal_receiver.next().await.unwrap();
    assert_eq!(receiver.next().await.unwrap(), ProposalPart::Init(ProposalInit::default()));
    let block_info = receiver.next().await.unwrap();
    let after: u64 =
        chrono::Utc::now().timestamp().try_into().expect("Timestamp conversion failed");
    let ProposalPart::BlockInfo(info) = block_info else {
        panic!("Expected ProposalPart::BlockInfo");
    };
    assert!(info.timestamp >= before && info.timestamp <= after);
    assert_eq!(info.eth_to_fri_rate, ETH_TO_FRI_RATE);
    assert_eq!(
        receiver.next().await.unwrap(),
        ProposalPart::Transactions(TransactionBatch { transactions: TX_BATCH.to_vec() })
    );
    assert_eq!(
        receiver.next().await.unwrap(),
        ProposalPart::ExecutedTransactionCount(INTERNAL_TX_BATCH.len().try_into().unwrap())
    );
    assert_eq!(
        receiver.next().await.unwrap(),
        ProposalPart::Fin(ProposalFin {
            proposal_commitment: BlockHash(STATE_DIFF_COMMITMENT.0.0),
        })
    );
    assert!(receiver.next().await.is_none());
    assert_eq!(fin_receiver.await.unwrap().0, STATE_DIFF_COMMITMENT.0.0);
}
#[tokio::test]
async fn build_proposal_cende_failure() {
    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_deps_for_build(BlockNumber(0), INTERNAL_TX_BATCH.len(), 1);
    let mut mock_cende_context = MockCendeContext::new();
    mock_cende_context
        .expect_write_prev_height_blob()
        .times(1)
        .return_once(|_height| tokio::spawn(ready(false)));
    deps.cende_ambassador = mock_cende_context;
    let mut context = deps.build_context();

    let fin_receiver = context.build_proposal(ProposalInit::default(), TIMEOUT).await;
    assert_eq!(fin_receiver.await, Err(Canceled));
}

#[tokio::test]
async fn build_proposal_cende_incomplete() {
    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_deps_for_build(BlockNumber(0), INTERNAL_TX_BATCH.len(), 1);
    let mut mock_cende_context = MockCendeContext::new();
    mock_cende_context
        .expect_write_prev_height_blob()
        .times(1)
        .return_once(|_height| tokio::spawn(pending()));
    deps.cende_ambassador = mock_cende_context;
    let mut context = deps.build_context();

    let fin_receiver = context.build_proposal(ProposalInit::default(), TIMEOUT).await;
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
    context.set_height_and_round(BlockNumber::default(), Round::default()).await;

    if proposer {
        let fin_receiver = context.build_proposal(ProposalInit::default(), TIMEOUT).await;
        assert_eq!(fin_receiver.await, Err(Canceled));
    } else {
        let (mut content_sender, content_receiver) =
            mpsc::channel(context.config.proposal_buffer_size);
        content_sender.send(ProposalPart::BlockInfo(block_info(BlockNumber(0)))).await.unwrap();

        let fin_receiver =
            context.validate_proposal(ProposalInit::default(), TIMEOUT, content_receiver).await;
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
    deps.setup_deps_for_build(BlockNumber(0), transactions.len(), 1);
    let mut context = deps.build_context();
    // Build proposal.
    let fin_receiver = context.build_proposal(ProposalInit::default(), TIMEOUT).await;
    let (_, mut receiver) = network.outbound_proposal_receiver.next().await.unwrap();
    // Receive the proposal parts.
    let _init = receiver.next().await.unwrap();
    let block_info = receiver.next().await.unwrap();
    let _txs = receiver.next().await.unwrap();
    let final_n_executed_txs = receiver.next().await.unwrap();
    assert!(matches!(final_n_executed_txs, ProposalPart::ExecutedTransactionCount(_)));
    let fin = receiver.next().await.unwrap();
    assert_eq!(fin_receiver.await.unwrap().0, STATE_DIFF_COMMITMENT.0.0);

    // Re-propose.
    context
        .repropose(
            BlockHash(STATE_DIFF_COMMITMENT.0.0),
            ProposalInit { round: 1, ..Default::default() },
        )
        .await;
    // Re-propose sends the same proposal.
    let (_, mut receiver) = network.outbound_proposal_receiver.next().await.unwrap();
    let _init = receiver.next().await.unwrap();
    assert_eq!(receiver.next().await.unwrap(), block_info);

    let reproposed_txs = ProposalPart::Transactions(TransactionBatch { transactions });
    assert_eq!(receiver.next().await.unwrap(), reproposed_txs);

    assert_eq!(receiver.next().await.unwrap(), final_n_executed_txs);
    assert_eq!(receiver.next().await.unwrap(), fin);
    assert!(receiver.next().await.is_none());
}

#[tokio::test]
async fn eth_to_fri_rate_out_of_range() {
    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_default_expectations();

    deps.batcher
        .expect_start_height()
        .times(1)
        .withf(|input| input.height == BlockNumber(0))
        .return_const(Ok(()));
    let mut context = deps.build_context();
    context.set_height_and_round(BlockNumber(0), 0).await;
    let (mut content_sender, content_receiver) = mpsc::channel(context.config.proposal_buffer_size);
    // Send a block info with an eth_to_fri_rate that is outside the margin of error.
    let mut block_info = block_info(BlockNumber(0));
    block_info.eth_to_fri_rate *= 2;
    content_sender.send(ProposalPart::BlockInfo(block_info).clone()).await.unwrap();
    // Use a large enough timeout to ensure fin_receiver was canceled due to invalid block_info,
    // not due to a timeout.
    let fin_receiver =
        context.validate_proposal(ProposalInit::default(), TIMEOUT * 100, content_receiver).await;
    assert_eq!(fin_receiver.await, Err(Canceled));
    // TODO(guyn): How to check that the rejection is due to the eth_to_fri_rate?
}

#[rstest]
#[case::maximum(true)]
#[case::minimum(false)]
#[tokio::test]
async fn gas_price_limits(#[case] maximum: bool) {
    let (mut deps, _network) = create_test_and_network_deps();
    deps.setup_deps_for_validate(BlockNumber(0), INTERNAL_TX_BATCH.len(), 1);
    let context_config = ContextConfig::default();
    let min_gas_price = context_config.min_l1_gas_price_wei;
    let min_data_price = context_config.min_l1_data_gas_price_wei;
    let max_gas_price = context_config.max_l1_gas_price_wei;
    let max_data_price = context_config.max_l1_data_gas_price_wei;

    let price = if maximum {
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
        Ok(PriceInfo { base_fee_per_gas: GasPrice(price), blob_fee: GasPrice(price) })
    });

    deps.l1_gas_price_provider = l1_gas_price_provider;
    let mut context = deps.build_context();

    context.set_height_and_round(BlockNumber(0), 0).await;
    let (mut content_sender, content_receiver) = mpsc::channel(context.config.proposal_buffer_size);

    let mut block_info = block_info(BlockNumber(0));

    if maximum {
        // Set the gas price to the maximum value.
        block_info.l1_gas_price_wei = GasPrice(max_gas_price);
        block_info.l1_data_gas_price_wei = GasPrice(max_data_price);
    } else {
        // Set the gas price to the minimum value.
        block_info.l1_gas_price_wei = GasPrice(min_gas_price);
        block_info.l1_data_gas_price_wei = GasPrice(min_data_price);
    }

    // Send the block info, some transactions and then fin.
    content_sender.send(ProposalPart::BlockInfo(block_info).clone()).await.unwrap();
    content_sender
        .send(ProposalPart::Transactions(TransactionBatch { transactions: TX_BATCH.to_vec() }))
        .await
        .unwrap();
    content_sender
        .send(ProposalPart::ExecutedTransactionCount(INTERNAL_TX_BATCH.len().try_into().unwrap()))
        .await
        .unwrap();
    content_sender
        .send(ProposalPart::Fin(ProposalFin {
            proposal_commitment: BlockHash(STATE_DIFF_COMMITMENT.0.0),
        }))
        .await
        .unwrap();

    // Even though we used the minimum/maximum gas price, not the values we gave the provider,
    // the proposal should be still be valid due to the clamping of limit prices.
    let fin_receiver =
        context.validate_proposal(ProposalInit::default(), TIMEOUT, content_receiver).await;
    assert_eq!(fin_receiver.await, Ok(BlockHash(STATE_DIFF_COMMITMENT.0.0)));
}

#[tokio::test]
async fn decision_reached_sends_correct_values() {
    let (mut deps, _network) = create_test_and_network_deps();

    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    // We need to create a valid proposal to call decision_reached on.
    //
    // 1. Build proposal setup starts.
    deps.setup_deps_for_build(BlockNumber(0), INTERNAL_TX_BATCH.len(), 1);

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
    let _fin = context.build_proposal(ProposalInit::default(), TIMEOUT).await.await;
    // At this point we should have a valid proposal in the context which contains the timestamp.

    let vote = Vote {
        // Currently this is the only field used by decision_reached.
        height: 0,
        ..Default::default()
    };

    context.decision_reached(BlockHash(STATE_DIFF_COMMITMENT.0.0), vec![vote]).await.unwrap();

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
    deps.setup_deps_for_build(BlockNumber(0), INTERNAL_TX_BATCH.len(), 1);

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

    let init = ProposalInit::default();

    let fin_receiver = context.build_proposal(init, TIMEOUT).await;

    let (_, mut receiver) = network.outbound_proposal_receiver.next().await.unwrap();

    assert_eq!(receiver.next().await.unwrap(), ProposalPart::Init(ProposalInit::default()));
    let block_info = receiver.next().await.unwrap();
    let ProposalPart::BlockInfo(info) = block_info else {
        panic!("Expected ProposalPart::BlockInfo");
    };

    let default_context_config = ContextConfig::default();
    assert_eq!(info.eth_to_fri_rate, DEFAULT_ETH_TO_FRI_RATE);
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
        ProposalPart::ExecutedTransactionCount(INTERNAL_TX_BATCH.len().try_into().unwrap())
    );
    assert_eq!(
        receiver.next().await.unwrap(),
        ProposalPart::Fin(ProposalFin {
            proposal_commitment: BlockHash(STATE_DIFF_COMMITMENT.0.0),
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
    deps.setup_deps_for_validate(BlockNumber(0), INTERNAL_TX_BATCH.len(), 1);
    deps.setup_deps_for_build(BlockNumber(1), INTERNAL_TX_BATCH.len(), 1);

    // set up batcher decision_reached
    deps.batcher.expect_decision_reached().times(1).return_once(|_| {
        Ok(DecisionReachedResponse {
            state_diff: ThinStateDiff::default(),
            l2_gas_used: GasAmount::default(),
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
            Err(L1GasPriceClientError::EthToStrkOracleClientError(
                EthToStrkOracleClientError::MissingFieldError("".to_string(), "".to_string()),
            ))
        });
        deps.l1_gas_price_provider = l1_prices_oracle_client;
    }

    let mut context = deps.build_context();

    // Validate block number 0.

    // Initialize the context for a specific height, starting with round 0.
    context.set_height_and_round(BlockNumber(0), 0).await;

    let (mut content_sender, content_receiver) = mpsc::channel(context.config.proposal_buffer_size);
    content_sender.send(ProposalPart::BlockInfo(block_info(BlockNumber(0)))).await.unwrap();
    content_sender
        .send(ProposalPart::Transactions(TransactionBatch { transactions: TX_BATCH.to_vec() }))
        .await
        .unwrap();
    content_sender
        .send(ProposalPart::ExecutedTransactionCount(INTERNAL_TX_BATCH.len().try_into().unwrap()))
        .await
        .unwrap();
    content_sender
        .send(ProposalPart::Fin(ProposalFin {
            proposal_commitment: BlockHash(STATE_DIFF_COMMITMENT.0.0),
        }))
        .await
        .unwrap();
    let fin_receiver =
        context.validate_proposal(ProposalInit::default(), TIMEOUT, content_receiver).await;
    content_sender.close_channel();
    let block_hash = fin_receiver.await.unwrap().0;
    assert_eq!(block_hash, STATE_DIFF_COMMITMENT.0.0);

    // Decision reached

    context
        .decision_reached(
            BlockHash(block_hash),
            vec![Vote { block_hash: Some(BlockHash(block_hash)), ..Default::default() }],
        )
        .await
        .unwrap();

    // Build proposal for block number 1.
    let init = ProposalInit { height: BlockNumber(1), ..Default::default() };

    let fin_receiver = context.build_proposal(init, TIMEOUT).await;

    let (_, mut receiver) = network.outbound_proposal_receiver.next().await.unwrap();

    assert_eq!(
        receiver.next().await.unwrap(),
        ProposalPart::Init(ProposalInit { height: BlockNumber(1), ..Default::default() })
    );
    let info = receiver.next().await.unwrap();
    let ProposalPart::BlockInfo(info) = info else {
        panic!("Expected ProposalPart::BlockInfo");
    };

    let previous_block_info = block_info(BlockNumber(0));

    assert_eq!(info.eth_to_fri_rate, previous_block_info.eth_to_fri_rate);
    assert_eq!(info.l1_gas_price_wei, previous_block_info.l1_gas_price_wei);
    assert_eq!(info.l1_data_gas_price_wei, previous_block_info.l1_data_gas_price_wei);

    assert_eq!(
        receiver.next().await.unwrap(),
        ProposalPart::Transactions(TransactionBatch { transactions: TX_BATCH.to_vec() })
    );
    assert_eq!(
        receiver.next().await.unwrap(),
        ProposalPart::ExecutedTransactionCount(INTERNAL_TX_BATCH.len().try_into().unwrap())
    );
    assert_eq!(
        receiver.next().await.unwrap(),
        ProposalPart::Fin(ProposalFin {
            proposal_commitment: BlockHash(STATE_DIFF_COMMITMENT.0.0),
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
// ETH_TO_FRI_RATE must be larger/equal to 10^18 (wei to eth conversion factor)
const LOW_OVERRIDE_CONVERSION_RATE: u128 = u128::pow(10, 19);

// If we use really low L2 gas price, the block will fail to build.
const LOW_OVERRIDE_L2_GAS_PRICE_FAIL: u128 = 1; // FRI

#[rstest]
#[case::dont_override_prices(None, None, None, None)]
#[case::override_l2_gas_price(Some(ODDLY_SPECIFIC_L2_GAS_PRICE), None, None, None)]
#[case::override_l1_gas_price(None, Some(ODDLY_SPECIFIC_L1_GAS_PRICE), None, None)]
#[case::override_l1_data_gas_price(None, None, Some(ODDLY_SPECIFIC_L1_DATA_GAS_PRICE), None)]
#[case::override_all_prices(
    Some(ODDLY_SPECIFIC_L2_GAS_PRICE),
    Some(ODDLY_SPECIFIC_L1_GAS_PRICE),
    Some(ODDLY_SPECIFIC_L1_DATA_GAS_PRICE),
    None
)]
#[case::override_everything(
    Some(ODDLY_SPECIFIC_L2_GAS_PRICE),
    Some(ODDLY_SPECIFIC_L1_GAS_PRICE),
    Some(ODDLY_SPECIFIC_L1_DATA_GAS_PRICE),
    Some(ODDLY_SPECIFIC_CONVERSION_RATE)
)]
#[case::low_overrides(
    Some(LOW_OVERRIDE_L2_GAS_PRICE),
    Some(1),
    Some(1),
    Some(LOW_OVERRIDE_CONVERSION_RATE)
)]
#[case::low_l2_gas_price_fail(
    Some(LOW_OVERRIDE_L2_GAS_PRICE_FAIL),
    None,
    None,
    Some(LOW_OVERRIDE_CONVERSION_RATE)
)]
#[tokio::test]
async fn override_prices_behavior(
    #[case] override_l2_gas_price_fri: Option<u128>,
    #[case] override_l1_gas_price_wei: Option<u128>,
    #[case] override_l1_data_gas_price_wei: Option<u128>,
    #[case] override_eth_to_fri_rate: Option<u128>,
) {
    let build_success = override_l2_gas_price_fri != Some(LOW_OVERRIDE_L2_GAS_PRICE_FAIL);

    // Use high gas usage to ensure the L2 gas price is high.
    let mock_l2_gas_used = VersionedConstants::latest_constants().max_block_size;

    let (mut deps, _network) = create_test_and_network_deps();

    // Setup dependencies and mocks.
    #[allow(clippy::as_conversions)]
    deps.setup_deps_for_build(BlockNumber(0), INTERNAL_TX_BATCH.len(), build_success as usize);
    deps.l1_gas_price_provider.expect_get_eth_to_fri_rate().returning(|_| Ok(ETH_TO_FRI_RATE));
    deps.batcher.expect_decision_reached().return_once(move |_| {
        Ok(DecisionReachedResponse {
            state_diff: ThinStateDiff::default(),
            l2_gas_used: mock_l2_gas_used,
            central_objects: CentralObjects::default(),
        })
    });

    deps.state_sync_client.expect_add_new_block().return_once(|_| Ok(()));
    deps.cende_ambassador.expect_prepare_blob_for_next_height().return_once(|_| Ok(()));

    let context_config = ContextConfig {
        override_l2_gas_price_fri,
        override_l1_gas_price_wei,
        override_l1_data_gas_price_wei,
        override_eth_to_fri_rate,
        ..Default::default()
    };
    let mut context = deps.build_context();
    context.config = context_config;

    let min_gas_price = VersionedConstants::latest_constants().min_gas_price.0;
    let gas_price_params = make_gas_price_params(&context.config);
    let mut expected_l1_prices = PriceInfo {
        base_fee_per_gas: GasPrice(TEMP_ETH_GAS_FEE_IN_WEI),
        blob_fee: GasPrice(TEMP_ETH_BLOB_GAS_FEE_IN_WEI),
    };
    apply_fee_transformations(&mut expected_l1_prices, &gas_price_params);

    // Run proposal and decision logic.
    let fin_result = context.build_proposal(ProposalInit::default(), TIMEOUT).await.await;

    // In cases where we expect the batcher to fail the block build.
    if !build_success {
        assert!(fin_result.is_err());
        return;
    }

    context
        .decision_reached(BlockHash(STATE_DIFF_COMMITMENT.0.0), vec![Vote::default()])
        .await
        .unwrap();

    let actual_l2_gas_price = context.l2_gas_price.0;

    let previous_block = context.previous_block_info.clone().unwrap();
    let actual_l1_gas_price = previous_block.l1_gas_price_wei.0;
    let actual_l1_data_gas_price = previous_block.l1_data_gas_price_wei.0;
    let actual_conversion_rate = previous_block.eth_to_fri_rate;

    if let Some(override_l2_gas_price) = override_l2_gas_price_fri {
        // In this case the L2 gas price must match the given override.
        assert_eq!(
            actual_l2_gas_price, override_l2_gas_price,
            "Expected L2 gas price ({}) to match override_l2_gas_price ({})",
            actual_l2_gas_price, override_l2_gas_price
        );
    } else {
        // In this case the regular L2 gas calculation takes place, and gives a higher price.
        assert!(
            actual_l2_gas_price > min_gas_price,
            "Expected L2 gas price ({}) > minimum l2 gas price ({}) due to high usage (EIP-1559)",
            actual_l2_gas_price,
            min_gas_price
        );
    }

    if let Some(override_l1_gas_price) = override_l1_gas_price_wei {
        assert_eq!(
            actual_l1_gas_price, override_l1_gas_price,
            "Expected L1 gas price ({}) to match input l1 gas price ({})",
            actual_l1_gas_price, override_l1_gas_price
        );
    } else {
        assert_eq!(
            actual_l1_gas_price, expected_l1_prices.base_fee_per_gas.0,
            "Expected L1 gas price ({}) to match input l1 gas price ({})",
            actual_l1_gas_price, expected_l1_prices.base_fee_per_gas.0
        );
    }

    if let Some(override_l1_data_gas_price) = override_l1_data_gas_price_wei {
        assert_eq!(
            actual_l1_data_gas_price, override_l1_data_gas_price,
            "Expected L1 data gas price ({}) to match input l1 data gas price ({})",
            actual_l1_data_gas_price, override_l1_data_gas_price
        );
    } else {
        assert_eq!(
            actual_l1_data_gas_price, expected_l1_prices.blob_fee.0,
            "Expected L1 data gas price ({}) to match input l1 data gas price ({})",
            actual_l1_data_gas_price, expected_l1_prices.blob_fee.0
        );
    }

    if let Some(override_eth_to_fri_rate) = override_eth_to_fri_rate {
        assert_eq!(
            actual_conversion_rate, override_eth_to_fri_rate,
            "Expected conversion rate ({}) to match input conversion rate ({})",
            actual_conversion_rate, override_eth_to_fri_rate
        );
    } else {
        // Note: the "default eth to fri rate" is actually just 10^18 (eth to wei).
        // This is set in the default expectations and is used by many other tests.
        // So we'll just assume that this is the "real" conversion rate, unless overriden.
        assert_eq!(
            actual_conversion_rate, ETH_TO_FRI_RATE,
            "Expected conversion rate ({}) to match default conversion rate ({})",
            actual_conversion_rate, ETH_TO_FRI_RATE
        );
    }
}
