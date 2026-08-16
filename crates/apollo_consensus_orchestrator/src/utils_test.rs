use std::sync::Arc;

use apollo_batcher_types::communication::{BatcherClientError, MockBatcherClient};
use apollo_batcher_types::errors::BatcherError;
use apollo_consensus_orchestrator_config::config::{
    ContextDynamicConfig,
    DEFAULT_MAX_ETH_TO_FRI_RATE_CHANGE_PPT,
};
use apollo_l1_gas_price_types::{MockL1GasPriceProviderClient, PriceInfo};
use apollo_protobuf::consensus::ProposalInit;
use apollo_state_sync_types::communication::StateSyncClientError;
use apollo_state_sync_types::errors::StateSyncError;
use assert_matches::assert_matches;
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use metrics_exporter_prometheus::PrometheusBuilder;
use rstest::rstest;
use starknet_api::block::{
    BlockHash,
    BlockHashAndNumber,
    BlockNumber,
    GasPrice,
    TEMP_ETH_BLOB_GAS_FEE_IN_WEI,
    TEMP_ETH_GAS_FEE_IN_WEI,
};
use starknet_types_core::felt::Felt;

use crate::build_proposal::ProposalBuildArguments;
use crate::cende::MockCendeContext;
use crate::dynamic_gas_price::PPT_DENOMINATOR;
use crate::metrics::{register_metrics, CONSENSUS_ETH_TO_FRI_RATE_CLAMPED};
use crate::test_utils::create_proposal_build_arguments;
use crate::utils::{
    get_l1_prices_in_fri_and_wei,
    get_l1_prices_in_fri_and_wei_and_conversion_rate,
    make_gas_price_params,
    retrospective_block_hash,
    verify_retrospective_state_commitment_infos,
    wait_for_retrospective_block_hash,
    L1PricesInFri,
    L1PricesInWei,
    PreviousProposalInitInfo,
    RetrospectiveBlockHashError,
    RetrospectiveStateCommitmentInfosError,
};

const CURRENT_BLOCK_NUMBER: BlockNumber = BlockNumber(STORED_BLOCK_HASH_BUFFER);
const RETRO_BLOCK_NUMBER: BlockNumber = BlockNumber(0);
const NEXT_HEIGHT_RETRO_BLOCK_NUMBER: BlockNumber =
    BlockNumber(CURRENT_BLOCK_NUMBER.0 + 1 - STORED_BLOCK_HASH_BUFFER);
// A recorder offset above the next height's retrospective block number means its commitment infos
// are stored; an offset equal to it means they are missing.
const STORED_HEIGHT_OFFSET: Option<BlockNumber> =
    Some(BlockNumber(NEXT_HEIGHT_RETRO_BLOCK_NUMBER.0 + 1));
const BEHIND_HEIGHT_OFFSET: Option<BlockNumber> = Some(NEXT_HEIGHT_RETRO_BLOCK_NUMBER);
const MUST_HAVE_BLOCK_HASH_FOR: BlockNumber = BlockNumber(1);
const RETRO_BLOCK_HASH: BlockHash = BlockHash(Felt::from_hex_unchecked("0x1234567890abcdef"));

const PROPOSAL_TIMESTAMP: u64 = 1_700_000_000;
// A gas price of one gwei keeps the previous block's implied rate exactly equal to the rate it was
// built with, for the rates used in these tests.
const PREVIOUS_L1_GAS_PRICE_WEI: GasPrice = GasPrice(u128::pow(10, 9));
const PREVIOUS_ETH_TO_FRI_RATE: u128 = 2 * u128::pow(10, 18);
// The band the shipped default puts around `PREVIOUS_ETH_TO_FRI_RATE`.
const MAX_ETH_TO_FRI_RATE_CHANGE: u128 =
    PREVIOUS_ETH_TO_FRI_RATE * DEFAULT_MAX_ETH_TO_FRI_RATE_CHANGE_PPT / PPT_DENOMINATOR;
const MAX_ETH_TO_FRI_RATE: u128 = PREVIOUS_ETH_TO_FRI_RATE + MAX_ETH_TO_FRI_RATE_CHANGE;
const MIN_ETH_TO_FRI_RATE: u128 = PREVIOUS_ETH_TO_FRI_RATE - MAX_ETH_TO_FRI_RATE_CHANGE;

async fn get_proposal_init(args: &ProposalBuildArguments) -> ProposalInit {
    let timestamp = args.deps.clock.unix_now();
    let (l1_prices_fri, l1_prices_wei) = get_l1_prices_in_fri_and_wei(
        args.deps.l1_gas_price_provider.clone(),
        timestamp,
        args.previous_proposal_init.as_ref(),
        &args.gas_price_params,
    )
    .await;

    ProposalInit {
        height: args.build_param.height,
        round: args.build_param.round,
        valid_round: args.build_param.valid_round,
        proposer: args.build_param.proposer,
        timestamp,
        builder: args.builder_address,
        l1_da_mode: args.l1_da_mode,
        l2_gas_price_fri: args.l2_gas_price,
        l1_gas_price_wei: l1_prices_wei.l1_gas_price,
        l1_data_gas_price_wei: l1_prices_wei.l1_data_gas_price,
        l1_gas_price_fri: l1_prices_fri.l1_gas_price,
        l1_data_gas_price_fri: l1_prices_fri.l1_data_gas_price,
        starknet_version: starknet_api::block::StarknetVersion::LATEST,
        version_constant_commitment: Default::default(),
        fee_proposal_fri: None,
    }
}

#[tokio::test]
async fn retrospective_block_hash_happy_flow() {
    let (mut test_proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    test_proposal_args.build_param.height = CURRENT_BLOCK_NUMBER;
    // Setup batcher client.
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == MUST_HAVE_BLOCK_HASH_FOR)
        .times(1)
        .returning(move |_| Ok(RETRO_BLOCK_HASH));
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETRO_BLOCK_NUMBER)
        .times(1)
        .returning(move |_| Ok(RETRO_BLOCK_HASH));
    // Setup state sync client.
    test_proposal_args
        .deps
        .state_sync_client
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETRO_BLOCK_NUMBER)
        .times(1)
        .returning(move |_| Ok(RETRO_BLOCK_HASH));

    let proposal_args: ProposalBuildArguments = test_proposal_args.into();
    let init = get_proposal_init(&proposal_args).await;
    let res = retrospective_block_hash(
        proposal_args.deps.batcher,
        proposal_args.deps.state_sync_client,
        &init,
        proposal_args.compare_retrospective_block_hash,
    )
    .await
    .unwrap();
    assert_eq!(
        res,
        Some(BlockHashAndNumber { number: RETRO_BLOCK_NUMBER, hash: RETRO_BLOCK_HASH })
    );
}

#[tokio::test]
async fn retrospective_block_hash_state_sync_error() {
    let (mut test_proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    test_proposal_args.build_param.height = CURRENT_BLOCK_NUMBER;
    // Setup batcher client to pass the must-have check.
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == MUST_HAVE_BLOCK_HASH_FOR)
        .times(1)
        .returning(move |_| Ok(RETRO_BLOCK_HASH));
    // Setup state sync client to return an error.
    test_proposal_args
        .deps
        .state_sync_client
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETRO_BLOCK_NUMBER)
        .returning(move |_| {
            Err(StateSyncClientError::StateSyncError(StateSyncError::BlockNotFound(
                RETRO_BLOCK_NUMBER,
            )))
        });

    let proposal_args: ProposalBuildArguments = test_proposal_args.into();
    let init = get_proposal_init(&proposal_args).await;
    let res = retrospective_block_hash(
        proposal_args.deps.batcher,
        proposal_args.deps.state_sync_client,
        &init,
        proposal_args.compare_retrospective_block_hash,
    )
    .await
    .unwrap_err();

    assert_matches!(res, RetrospectiveBlockHashError::StateSyncError(_));
}

#[tokio::test]
async fn retrospective_block_hash_batcher_error() {
    let (mut test_proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    test_proposal_args.build_param.height = CURRENT_BLOCK_NUMBER;
    // Setup state sync client to return block hash.
    test_proposal_args
        .deps
        .state_sync_client
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETRO_BLOCK_NUMBER)
        .returning(move |_| Ok(RETRO_BLOCK_HASH));
    // Setup batcher client to pass the must-have check, then return an error for the retro block.
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == MUST_HAVE_BLOCK_HASH_FOR)
        .times(1)
        .returning(move |_| Ok(RETRO_BLOCK_HASH));
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETRO_BLOCK_NUMBER)
        .returning(move |_| {
            Err(BatcherClientError::BatcherError(BatcherError::BlockHashNotFound(
                RETRO_BLOCK_NUMBER,
            )))
        });

    let proposal_args: ProposalBuildArguments = test_proposal_args.into();
    let init = get_proposal_init(&proposal_args).await;
    let res = retrospective_block_hash(
        proposal_args.deps.batcher,
        proposal_args.deps.state_sync_client,
        &init,
        proposal_args.compare_retrospective_block_hash,
    )
    .await
    .unwrap_err();

    assert_matches!(res, RetrospectiveBlockHashError::BatcherError(_));
}

#[tokio::test]
async fn retrospective_block_hash_mismatch() {
    let (mut test_proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    test_proposal_args.build_param.height = CURRENT_BLOCK_NUMBER;
    // Setup state sync client to return block hash.
    test_proposal_args
        .deps
        .state_sync_client
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETRO_BLOCK_NUMBER)
        .returning(|_| Ok(RETRO_BLOCK_HASH));
    // Setup batcher client to pass the must-have check, then return a mismatched hash.
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == MUST_HAVE_BLOCK_HASH_FOR)
        .times(1)
        .returning(|_| Ok(RETRO_BLOCK_HASH));
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETRO_BLOCK_NUMBER)
        .returning(|_| Ok(BlockHash(Felt::ZERO)));

    let proposal_args: ProposalBuildArguments = test_proposal_args.into();
    let init = get_proposal_init(&proposal_args).await;
    let res = retrospective_block_hash(
        proposal_args.deps.batcher,
        proposal_args.deps.state_sync_client,
        &init,
        proposal_args.compare_retrospective_block_hash,
    )
    .await
    .unwrap_err();

    assert!(matches!(res, RetrospectiveBlockHashError::HashMismatch { .. }));
}

#[tokio::test]
async fn wait_for_retrospective_block_hash_state_sync_ready_after_a_while() {
    let (mut test_proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    test_proposal_args.build_param.height = CURRENT_BLOCK_NUMBER;
    // Setup batcher client to pass the must-have check (called once per loop iteration = 2 times),
    // then return the retro block hash once (only reached on the second iteration).
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == MUST_HAVE_BLOCK_HASH_FOR)
        .times(2)
        .returning(|_| Ok(RETRO_BLOCK_HASH));
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETRO_BLOCK_NUMBER)
        .returning(|_| Ok(RETRO_BLOCK_HASH));
    // Setup state sync client to return BlockNotFound error in the first attempt.
    test_proposal_args
        .deps
        .state_sync_client
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETRO_BLOCK_NUMBER)
        .times(1)
        .returning(|_| Err(StateSyncError::BlockNotFound(RETRO_BLOCK_NUMBER).into()));
    // Setup state sync client to return a block hash in the second attempt.
    test_proposal_args
        .deps
        .state_sync_client
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETRO_BLOCK_NUMBER)
        .times(1)
        .returning(|_| Ok(RETRO_BLOCK_HASH));

    let proposal_args: ProposalBuildArguments = test_proposal_args.into();
    let init = get_proposal_init(&proposal_args).await;
    let res = wait_for_retrospective_block_hash(
        proposal_args.deps.batcher,
        proposal_args.deps.state_sync_client,
        &init,
        proposal_args.deps.clock.as_ref(),
        proposal_args.retrospective_block_hash_deadline,
        proposal_args.retrospective_block_hash_retry_interval_millis,
        proposal_args.compare_retrospective_block_hash,
    )
    .await
    .unwrap();
    assert_eq!(
        res,
        Some(BlockHashAndNumber { number: RETRO_BLOCK_NUMBER, hash: RETRO_BLOCK_HASH })
    );
}

#[tokio::test]
async fn wait_for_retrospective_block_hash_batcher_ready_after_a_while() {
    let (mut test_proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    test_proposal_args.build_param.height = CURRENT_BLOCK_NUMBER;
    // Setup state sync client to return block hash in both attempts.
    test_proposal_args
        .deps
        .state_sync_client
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETRO_BLOCK_NUMBER)
        .times(2)
        .returning(|_| Ok(RETRO_BLOCK_HASH));
    // Setup batcher client to pass the must-have check (called once per loop iteration = 2 times).
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == MUST_HAVE_BLOCK_HASH_FOR)
        .times(2)
        .returning(|_| Ok(RETRO_BLOCK_HASH));
    // Setup batcher client to return BlockHashNotFound error in the first attempt.
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETRO_BLOCK_NUMBER)
        .times(1)
        .returning(|_| {
            Err(BatcherClientError::BatcherError(BatcherError::BlockHashNotFound(
                RETRO_BLOCK_NUMBER,
            )))
        });
    // Setup batcher client to return a block hash in the second attempt.
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETRO_BLOCK_NUMBER)
        .times(1)
        .returning(|_| Ok(RETRO_BLOCK_HASH));

    let proposal_args: ProposalBuildArguments = test_proposal_args.into();
    let init = get_proposal_init(&proposal_args).await;
    let res = wait_for_retrospective_block_hash(
        proposal_args.deps.batcher,
        proposal_args.deps.state_sync_client,
        &init,
        proposal_args.deps.clock.as_ref(),
        proposal_args.retrospective_block_hash_deadline,
        proposal_args.retrospective_block_hash_retry_interval_millis,
        proposal_args.compare_retrospective_block_hash,
    )
    .await
    .unwrap();
    assert_eq!(
        res,
        Some(BlockHashAndNumber { number: RETRO_BLOCK_NUMBER, hash: RETRO_BLOCK_HASH })
    );
}

fn mock_batcher_commitment_infos(batcher_has_infos: bool) -> MockBatcherClient {
    let mut batcher = MockBatcherClient::new();
    batcher.expect_has_state_commitment_infos().times(1).returning(move |block_number| {
        assert_eq!(block_number, NEXT_HEIGHT_RETRO_BLOCK_NUMBER);
        Ok(batcher_has_infos)
    });
    batcher
}

fn mock_cende_recorder_height_offset(height_offset: Option<BlockNumber>) -> MockCendeContext {
    let mut cende_ambassador = MockCendeContext::new();
    cende_ambassador
        .expect_commitment_infos_height_offset()
        .times(1)
        .returning(move || Ok(height_offset));
    cende_ambassador
}

#[rstest]
#[case::stored_on_batcher(true, None, true)]
#[case::stored_only_on_cende(false, STORED_HEIGHT_OFFSET, true)]
#[case::both_sides_empty_skip_validation(false, None, true)]
#[case::missing_on_batcher_and_recorder_behind(false, BEHIND_HEIGHT_OFFSET, false)]
#[tokio::test]
async fn retrospective_state_commitment_infos(
    #[case] batcher_has_infos: bool,
    #[case] cende_recorder_height_offset: Option<BlockNumber>,
    #[case] validation_passes: bool,
) {
    // When the batcher has the commitment infos, the cende recorder must not be queried.
    let cende_ambassador = if batcher_has_infos {
        MockCendeContext::new()
    } else {
        mock_cende_recorder_height_offset(cende_recorder_height_offset)
    };
    let res = verify_retrospective_state_commitment_infos(
        &mock_batcher_commitment_infos(batcher_has_infos),
        &cende_ambassador,
        CURRENT_BLOCK_NUMBER,
    )
    .await;

    if validation_passes {
        res.unwrap();
    } else {
        assert_matches!(res.unwrap_err(), RetrospectiveStateCommitmentInfosError::NotStored { .. });
    }
}

#[tokio::test]
async fn retrospective_state_commitment_infos_next_height_below_buffer() {
    // No queries are expected: heights whose next height is below the buffer have no
    // retrospective block.
    verify_retrospective_state_commitment_infos(
        &MockBatcherClient::new(),
        &MockCendeContext::new(),
        BlockNumber(STORED_BLOCK_HASH_BUFFER - 2),
    )
    .await
    .unwrap();
}

/// Builds the previous block's recorded prices such that they imply `eth_to_fri_rate`.
fn previous_proposal_init_with_rate(eth_to_fri_rate: u128) -> PreviousProposalInitInfo {
    let l1_prices_wei = L1PricesInWei {
        l1_gas_price: PREVIOUS_L1_GAS_PRICE_WEI,
        l1_data_gas_price: PREVIOUS_L1_GAS_PRICE_WEI,
    };
    let l1_prices_fri = L1PricesInFri::convert_from_wei(&l1_prices_wei, eth_to_fri_rate)
        .expect("Test prices should be convertible to fri.");
    PreviousProposalInitInfo { timestamp: PROPOSAL_TIMESTAMP, l1_prices_wei, l1_prices_fri }
}

/// Fetches the eth to fri rate through the full oracle path, with a freshly built provider and a
/// freshly built config, both independent of any other instance.
async fn fetch_eth_to_fri_rate(
    oracle_eth_to_fri_rate: u128,
    previous_proposal_init: Option<&PreviousProposalInitInfo>,
) -> u128 {
    fetch_eth_to_fri_rate_with_config(
        oracle_eth_to_fri_rate,
        previous_proposal_init,
        ContextDynamicConfig::default(),
    )
    .await
}

async fn fetch_eth_to_fri_rate_with_config(
    oracle_eth_to_fri_rate: u128,
    previous_proposal_init: Option<&PreviousProposalInitInfo>,
    dynamic_config: ContextDynamicConfig,
) -> u128 {
    let mut l1_gas_price_provider = MockL1GasPriceProviderClient::new();
    l1_gas_price_provider.expect_get_rate().return_const(Ok(oracle_eth_to_fri_rate));
    l1_gas_price_provider.expect_get_price_info().return_const(Ok(PriceInfo {
        base_fee_per_gas: GasPrice(TEMP_ETH_GAS_FEE_IN_WEI),
        blob_fee: GasPrice(TEMP_ETH_BLOB_GAS_FEE_IN_WEI),
    }));

    let (_l1_prices_fri, _l1_prices_wei, eth_to_fri_rate) =
        get_l1_prices_in_fri_and_wei_and_conversion_rate(
            Arc::new(l1_gas_price_provider),
            PROPOSAL_TIMESTAMP,
            previous_proposal_init,
            &make_gas_price_params(&dynamic_config),
        )
        .await;
    eth_to_fri_rate
}

#[tokio::test]
async fn eth_to_fri_rate_inside_the_band_is_not_clamped() {
    let previous_proposal_init = previous_proposal_init_with_rate(PREVIOUS_ETH_TO_FRI_RATE);
    let oracle_eth_to_fri_rate = PREVIOUS_ETH_TO_FRI_RATE + MAX_ETH_TO_FRI_RATE_CHANGE / 2;
    assert_eq!(
        fetch_eth_to_fri_rate(oracle_eth_to_fri_rate, Some(&previous_proposal_init)).await,
        oracle_eth_to_fri_rate
    );
}

#[tokio::test]
async fn eth_to_fri_rate_above_the_band_is_clamped_to_the_maximum() {
    let previous_proposal_init = previous_proposal_init_with_rate(PREVIOUS_ETH_TO_FRI_RATE);
    assert_eq!(
        fetch_eth_to_fri_rate(PREVIOUS_ETH_TO_FRI_RATE * 3, Some(&previous_proposal_init)).await,
        MAX_ETH_TO_FRI_RATE
    );
}

#[tokio::test]
async fn eth_to_fri_rate_below_the_band_is_clamped_to_the_minimum() {
    let previous_proposal_init = previous_proposal_init_with_rate(PREVIOUS_ETH_TO_FRI_RATE);
    assert_eq!(
        fetch_eth_to_fri_rate(PREVIOUS_ETH_TO_FRI_RATE / 3, Some(&previous_proposal_init)).await,
        MIN_ETH_TO_FRI_RATE
    );
}

#[tokio::test]
async fn eth_to_fri_rate_exactly_at_the_band_edge_is_not_clamped() {
    let previous_proposal_init = previous_proposal_init_with_rate(PREVIOUS_ETH_TO_FRI_RATE);
    assert_eq!(
        fetch_eth_to_fri_rate(MAX_ETH_TO_FRI_RATE, Some(&previous_proposal_init)).await,
        MAX_ETH_TO_FRI_RATE
    );
    assert_eq!(
        fetch_eth_to_fri_rate(MIN_ETH_TO_FRI_RATE, Some(&previous_proposal_init)).await,
        MIN_ETH_TO_FRI_RATE
    );
}

/// Genesis is the only height with no `previous_proposal_init`. A restarted node seeds it from
/// state_sync, covered by `test_initialize_from_committed_blocks_seeds_previous_block`.
#[tokio::test]
async fn eth_to_fri_rate_at_genesis_is_not_clamped() {
    let oracle_eth_to_fri_rate = PREVIOUS_ETH_TO_FRI_RATE * 100;
    assert_eq!(fetch_eth_to_fri_rate(oracle_eth_to_fri_rate, None).await, oracle_eth_to_fri_rate);
}

#[tokio::test]
async fn eth_to_fri_rate_clamp_metric_increments_only_when_clamping() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    register_metrics();

    let previous_proposal_init = previous_proposal_init_with_rate(PREVIOUS_ETH_TO_FRI_RATE);
    fetch_eth_to_fri_rate(PREVIOUS_ETH_TO_FRI_RATE, Some(&previous_proposal_init)).await;
    CONSENSUS_ETH_TO_FRI_RATE_CLAMPED.assert_eq(&recorder.handle().render(), 0);

    fetch_eth_to_fri_rate(PREVIOUS_ETH_TO_FRI_RATE * 3, Some(&previous_proposal_init)).await;
    CONSENSUS_ETH_TO_FRI_RATE_CLAMPED.assert_eq(&recorder.handle().render(), 1);
}

/// The operator pin replaces the rate the clamp returns, so a clamp there would shape no published
/// price. `get_l1_prices_in_fri_and_wei_and_conversion_rate` returns the rate before the pin is
/// applied, so the assertion is that the oracle rate passed through untouched and uncounted.
#[tokio::test]
async fn eth_to_fri_rate_is_not_clamped_when_the_operator_pins_the_rate() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    register_metrics();

    let previous_proposal_init = previous_proposal_init_with_rate(PREVIOUS_ETH_TO_FRI_RATE);
    let oracle_eth_to_fri_rate = PREVIOUS_ETH_TO_FRI_RATE * 3;
    let dynamic_config = ContextDynamicConfig {
        override_eth_to_fri_rate: Some(PREVIOUS_ETH_TO_FRI_RATE),
        ..Default::default()
    };

    assert_eq!(
        fetch_eth_to_fri_rate_with_config(
            oracle_eth_to_fri_rate,
            Some(&previous_proposal_init),
            dynamic_config
        )
        .await,
        oracle_eth_to_fri_rate
    );
    CONSENSUS_ETH_TO_FRI_RATE_CLAMPED.assert_eq(&recorder.handle().render(), 0);
}

/// A previous block whose prices imply no rate leaves the clamp with nothing to center on, so the
/// oracle rate passes through unclamped and uncounted.
#[rstest]
#[case::zero_previous_wei_price(GasPrice(0), GasPrice(u128::pow(10, 9)))]
#[case::overflowing_previous_fri_price(PREVIOUS_L1_GAS_PRICE_WEI, GasPrice(u128::MAX))]
#[tokio::test]
async fn eth_to_fri_rate_is_not_clamped_when_the_previous_block_implies_no_rate(
    #[case] previous_l1_gas_price_wei: GasPrice,
    #[case] previous_l1_gas_price_fri: GasPrice,
) {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    register_metrics();

    let previous_proposal_init = PreviousProposalInitInfo {
        timestamp: PROPOSAL_TIMESTAMP,
        l1_prices_wei: L1PricesInWei {
            l1_gas_price: previous_l1_gas_price_wei,
            l1_data_gas_price: previous_l1_gas_price_wei,
        },
        l1_prices_fri: L1PricesInFri {
            l1_gas_price: previous_l1_gas_price_fri,
            l1_data_gas_price: previous_l1_gas_price_fri,
        },
    };
    let oracle_eth_to_fri_rate = PREVIOUS_ETH_TO_FRI_RATE * 100;

    assert_eq!(
        fetch_eth_to_fri_rate(oracle_eth_to_fri_rate, Some(&previous_proposal_init)).await,
        oracle_eth_to_fri_rate
    );
    CONSENSUS_ETH_TO_FRI_RATE_CLAMPED.assert_eq(&recorder.handle().render(), 0);
}
