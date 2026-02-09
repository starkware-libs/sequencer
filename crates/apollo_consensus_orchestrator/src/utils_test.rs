use apollo_batcher_types::communication::{BatcherClientError, BatcherClientResult};
use apollo_batcher_types::errors::BatcherError;
use apollo_infra::component_client::ClientError;
use apollo_protobuf::consensus::ProposalInit;
use apollo_state_sync_types::communication::{StateSyncClientError, StateSyncClientResult};
use apollo_state_sync_types::errors::StateSyncError;
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use metrics_exporter_prometheus::PrometheusBuilder;
use rstest::rstest;
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};
use starknet_types_core::felt::Felt;

use crate::build_proposal::ProposalBuildArguments;
use crate::metrics::CONSENSUS_RETROSPECTIVE_BLOCK_HASH_FROM_STATE_SYNC;
use crate::test_utils::create_proposal_build_arguments;
use crate::utils::{
    get_l1_prices_in_fri_and_wei,
    retrospective_block_hash,
    wait_for_retrospective_block_hash,
    RetrospectiveBlockHashError,
};

const CURRENT_BLOCK_NUMBER: BlockNumber = BlockNumber(STORED_BLOCK_HASH_BUFFER);
const RETRO_BLOCK_NUMBER: BlockNumber = BlockNumber(0);
const RETRO_BLOCK_HASH: BlockHash = BlockHash(Felt::from_hex_unchecked("0x1234567890abcdef"));

async fn get_block_info(args: &ProposalBuildArguments) -> ProposalInit {
    let timestamp = args.deps.clock.unix_now();
    let (l1_prices_fri, l1_prices_wei) = get_l1_prices_in_fri_and_wei(
        args.deps.l1_gas_price_provider.clone(),
        timestamp,
        args.previous_block_info.as_ref(),
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
    }
}

#[rstest]
#[case::batcher_succeeds(Ok(RETRO_BLOCK_HASH), None)]
#[case::batcher_error_state_sync_succeeds(
    Err(BatcherClientError::BatcherError(BatcherError::InternalError)),
    Some(Ok(RETRO_BLOCK_HASH))
)]
#[tokio::test]
async fn retrospective_block_hash_happy_flow(
    #[case] batcher_result: BatcherClientResult<BlockHash>,
    #[case] state_sync_result: Option<StateSyncClientResult<BlockHash>>,
) {
    let recorder = PrometheusBuilder::new().build_recorder();
    let _recorder_guard = metrics::set_default_local_recorder(&recorder);
    let (mut test_proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    test_proposal_args.build_param.height = CURRENT_BLOCK_NUMBER;
    // Setup batcher client.
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETRO_BLOCK_NUMBER)
        .times(1)
        .returning(move |_| batcher_result.clone());
    // Setup state sync client and set metrics value only if batcher fails.
    let metrics_value = state_sync_result.map(|state_sync_result| {
        test_proposal_args
            .deps
            .state_sync_client
            .expect_get_block_hash()
            .withf(|block_number| *block_number == RETRO_BLOCK_NUMBER)
            .times(1)
            .returning(move |_| state_sync_result.clone());
        1
    });

    let proposal_args: ProposalBuildArguments = test_proposal_args.into();
    let init = get_block_info(&proposal_args).await;
    let res = retrospective_block_hash(
        proposal_args.deps.batcher,
        proposal_args.deps.state_sync_client,
        &init,
    )
    .await
    .unwrap();
    assert_eq!(
        res,
        Some(BlockHashAndNumber { number: RETRO_BLOCK_NUMBER, hash: RETRO_BLOCK_HASH })
    );
    assert_eq!(
        CONSENSUS_RETROSPECTIVE_BLOCK_HASH_FROM_STATE_SYNC
            .parse_numeric_metric::<u64>(&recorder.handle().render()),
        metrics_value
    );
}

#[rstest]
#[case::both_not_ready(
    BatcherClientError::BatcherError(BatcherError::BlockHashNotFound(RETRO_BLOCK_NUMBER)),
    StateSyncClientError::StateSyncError(StateSyncError::BlockNotFound(RETRO_BLOCK_NUMBER))
)]
#[case::batcher_not_ready_state_sync_fails(
    BatcherClientError::BatcherError(BatcherError::BlockHashNotFound(RETRO_BLOCK_NUMBER)),
    StateSyncClientError::ClientError(ClientError::CommunicationFailure("".to_string()))
)]
#[case::batcher_fails_state_sync_not_ready(
    BatcherClientError::BatcherError(BatcherError::InternalError),
    StateSyncClientError::StateSyncError(StateSyncError::BlockNotFound(RETRO_BLOCK_NUMBER))
)]
#[case::both_fail(
    BatcherClientError::BatcherError(BatcherError::InternalError),
    StateSyncClientError::ClientError(ClientError::CommunicationFailure("".to_string()))
)]
#[tokio::test]
async fn retrospective_block_hash_sad_flow(
    #[case] batcher_error: BatcherClientError,
    #[case] state_sync_error: StateSyncClientError,
) {
    let (mut test_proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    test_proposal_args.build_param.height = CURRENT_BLOCK_NUMBER;
    // Clone the errors to use them in the match statement.
    let batcher_error_cloned = batcher_error.clone();
    let state_sync_error_cloned = state_sync_error.clone();
    // Setup batcher client to return an error.
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETRO_BLOCK_NUMBER)
        .returning(move |_| Err(batcher_error.clone()));
    // Setup state sync client to return an error.
    test_proposal_args
        .deps
        .state_sync_client
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETRO_BLOCK_NUMBER)
        .returning(move |_| Err(state_sync_error.clone()));

    let proposal_args: ProposalBuildArguments = test_proposal_args.into();
    let init = get_block_info(&proposal_args).await;
    let res = retrospective_block_hash(
        proposal_args.deps.batcher,
        proposal_args.deps.state_sync_client,
        &init,
    )
    .await
    .unwrap_err();

    match (&batcher_error_cloned, &state_sync_error_cloned) {
        (BatcherClientError::BatcherError(BatcherError::BlockHashNotFound(_)), _)
        | (_, StateSyncClientError::StateSyncError(StateSyncError::BlockNotFound(_))) => {
            assert!(matches!(res, RetrospectiveBlockHashError::NotReady { .. }));
        }
        (_, _) => {
            assert!(matches!(res, RetrospectiveBlockHashError::FailedRetrievingHash { .. }));
        }
    }
}

#[tokio::test]
async fn wait_for_retrospective_block_hash_state_sync_ready_after_a_while() {
    let (mut test_proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    test_proposal_args.build_param.height = CURRENT_BLOCK_NUMBER;
    // Setup batcher client to return an error.
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETRO_BLOCK_NUMBER)
        .returning(|_| Err(BatcherClientError::BatcherError(BatcherError::InternalError)));
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
    let init = get_block_info(&proposal_args).await;
    let res = wait_for_retrospective_block_hash(
        proposal_args.deps.batcher,
        proposal_args.deps.state_sync_client,
        &init,
        proposal_args.deps.clock.as_ref(),
        proposal_args.retrospective_block_hash_deadline,
        proposal_args.retrospective_block_hash_retry_interval_millis,
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
    // Setup state sync client to return BlockNotFound error.
    test_proposal_args
        .deps
        .state_sync_client
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETRO_BLOCK_NUMBER)
        .returning(|_| Err(StateSyncError::BlockNotFound(RETRO_BLOCK_NUMBER).into()));

    let proposal_args: ProposalBuildArguments = test_proposal_args.into();
    let init = get_block_info(&proposal_args).await;
    let res = wait_for_retrospective_block_hash(
        proposal_args.deps.batcher,
        proposal_args.deps.state_sync_client,
        &init,
        proposal_args.deps.clock.as_ref(),
        proposal_args.retrospective_block_hash_deadline,
        proposal_args.retrospective_block_hash_retry_interval_millis,
    )
    .await
    .unwrap();
    assert_eq!(
        res,
        Some(BlockHashAndNumber { number: RETRO_BLOCK_NUMBER, hash: RETRO_BLOCK_HASH })
    );
}
