use apollo_batcher_types::communication::{BatcherClientError, BatcherClientResult};
use apollo_batcher_types::errors::BatcherError;
use apollo_infra::component_client::ClientError;
use apollo_protobuf::consensus::ConsensusBlockInfo;
use apollo_state_sync_types::communication::{StateSyncClientError, StateSyncClientResult};
use apollo_state_sync_types::errors::StateSyncError;
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use rstest::rstest;
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};

use crate::build_proposal::ProposalBuildArguments;
use crate::test_utils::create_proposal_build_arguments;
use crate::utils::{
    get_oracle_rate_and_prices,
    retrospective_block_hash,
    wait_for_retrospective_block_hash,
    RetrospectiveBlockHashError,
};

const CURRENT_BLOCK_NUMBER: BlockNumber = BlockNumber(STORED_BLOCK_HASH_BUFFER);
const RETTRO_BLOCK_NUMBER: BlockNumber = BlockNumber(0);

async fn get_block_info(args: &ProposalBuildArguments) -> ConsensusBlockInfo {
    let timestamp = args.deps.clock.unix_now();
    let (eth_to_fri_rate, l1_prices) = get_oracle_rate_and_prices(
        args.deps.l1_gas_price_provider.clone(),
        timestamp,
        args.previous_block_info.as_ref(),
        &args.gas_price_params,
    )
    .await;

    ConsensusBlockInfo {
        height: args.proposal_init.height,
        timestamp,
        builder: args.builder_address,
        l1_da_mode: args.l1_da_mode,
        l2_gas_price_fri: args.l2_gas_price,
        l1_gas_price_wei: l1_prices.base_fee_per_gas,
        l1_data_gas_price_wei: l1_prices.blob_fee,
        eth_to_fri_rate,
    }
}

#[tokio::test]
async fn wait_for_retrospective_block_hash_batcher_succeeds() {
    let (mut test_proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    test_proposal_args.proposal_init.height = CURRENT_BLOCK_NUMBER;
    // Setup batcher client to return a block hash for the retrospective block number.
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETTRO_BLOCK_NUMBER)
        .times(1)
        .returning(|_| Ok(BlockHash::default()));
    // No need to setup state sync client, as the batcher client will return a block hash.

    let proposal_args: ProposalBuildArguments = test_proposal_args.into();
    let block_info = get_block_info(&proposal_args).await;
    let res = wait_for_retrospective_block_hash(
        proposal_args.deps.batcher,
        proposal_args.deps.state_sync_client,
        &block_info,
        proposal_args.deps.clock.as_ref(),
        proposal_args.retrospective_block_hash_deadline,
        proposal_args.retrospective_block_hash_retry_interval_millis,
    )
    .await
    .unwrap();
    assert_eq!(
        res,
        Some(BlockHashAndNumber { number: RETTRO_BLOCK_NUMBER, hash: BlockHash::default() })
    );
}

#[tokio::test]
async fn wait_for_retrospective_block_hash_batcher_error_state_sync_succeed() {
    let (mut test_proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    test_proposal_args.proposal_init.height = CURRENT_BLOCK_NUMBER;
    // Setup batcher client to return an error.
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETTRO_BLOCK_NUMBER)
        .times(1)
        .returning(|_| Err(BatcherClientError::BatcherError(BatcherError::InternalError)));
    // Setup state sync client to return a block hash.
    test_proposal_args
        .deps
        .state_sync_client
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETTRO_BLOCK_NUMBER)
        .returning(|_| Ok(BlockHash::default()));

    let proposal_args: ProposalBuildArguments = test_proposal_args.into();
    let block_info = get_block_info(&proposal_args).await;
    let res = wait_for_retrospective_block_hash(
        proposal_args.deps.batcher,
        proposal_args.deps.state_sync_client,
        &block_info,
        proposal_args.deps.clock.as_ref(),
        proposal_args.retrospective_block_hash_deadline,
        proposal_args.retrospective_block_hash_retry_interval_millis,
    )
    .await
    .unwrap();
    assert_eq!(
        res,
        Some(BlockHashAndNumber { number: RETTRO_BLOCK_NUMBER, hash: BlockHash::default() })
    );
}

#[tokio::test]
async fn wait_for_retrospective_block_hash_both_fail() {
    let (mut test_proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    test_proposal_args.proposal_init.height = CURRENT_BLOCK_NUMBER;
    // Setup batcher client to return an error.
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETTRO_BLOCK_NUMBER)
        .times(1)
        .returning(|_| Err(BatcherClientError::BatcherError(BatcherError::InternalError)));
    // Setup state sync client to return an error.
    test_proposal_args
        .deps
        .state_sync_client
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETTRO_BLOCK_NUMBER)
        .times(1)
        .returning(|_| {
            Err(StateSyncClientError::ClientError(ClientError::CommunicationFailure(
                "".to_string(),
            )))
        });

    let proposal_args: ProposalBuildArguments = test_proposal_args.into();
    let block_info = get_block_info(&proposal_args).await;
    let res = wait_for_retrospective_block_hash(
        proposal_args.deps.batcher,
        proposal_args.deps.state_sync_client,
        &block_info,
        proposal_args.deps.clock.as_ref(),
        proposal_args.retrospective_block_hash_deadline,
        proposal_args.retrospective_block_hash_retry_interval_millis,
    )
    .await;
    assert!(matches!(res, Err(RetrospectiveBlockHashError::FailedRetrievingHash { .. })));
}

#[rstest]
#[case::batcher_succeeds_state_sync_not_ready(
    Ok(BlockHash::default()),
    Err(StateSyncClientError::StateSyncError(StateSyncError::BlockNotFound(
        BlockNumber::default()
    )))
)]
#[case::batcher_error_state_sync_succeeds(
    Err(BatcherClientError::BatcherError(BatcherError::InternalError)),
    Ok(BlockHash::default())
)]
#[case::both_succeed(Ok(BlockHash::default()), Ok(BlockHash::default()))]
#[tokio::test]
async fn retrospective_block_hash_happy_flow(
    #[case] batcher_result: BatcherClientResult<BlockHash>,
    #[case] state_sync_result: StateSyncClientResult<BlockHash>,
) {
    let (mut test_proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    test_proposal_args.proposal_init.height = CURRENT_BLOCK_NUMBER;
    // Clone batcher result for use in match statement after closures capture the original.
    let batcher_result_cloned = batcher_result.clone();
    // Setup batcher client.
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETTRO_BLOCK_NUMBER)
        .times(1)
        .returning(move |_| batcher_result.clone());
    // Setup state sync client only if batcher fails.
    if batcher_result_cloned.is_err() {
        test_proposal_args
            .deps
            .state_sync_client
            .expect_get_block_hash()
            .withf(|block_number| *block_number == RETTRO_BLOCK_NUMBER)
            .times(1)
            .returning(move |_| state_sync_result.clone());
    }

    let proposal_args: ProposalBuildArguments = test_proposal_args.into();
    let block_info = get_block_info(&proposal_args).await;
    let res = retrospective_block_hash(
        proposal_args.deps.batcher,
        proposal_args.deps.state_sync_client,
        &block_info,
    )
    .await
    .unwrap();
    assert_eq!(
        res,
        Some(BlockHashAndNumber { number: RETTRO_BLOCK_NUMBER, hash: BlockHash::default() })
    );
}

#[rstest]
#[case::both_not_ready(
    BatcherClientError::BatcherError(BatcherError::BlockHashNotFound(BlockNumber::default())),
    StateSyncClientError::StateSyncError(StateSyncError::BlockNotFound(BlockNumber::default()))
)]
#[case::batcher_not_ready_state_sync_fails(
    BatcherClientError::BatcherError(BatcherError::BlockHashNotFound(BlockNumber::default())),
    StateSyncClientError::ClientError(ClientError::CommunicationFailure("".to_string())),
)]
#[case::batcher_fails_state_sync_not_ready(
    BatcherClientError::BatcherError(BatcherError::InternalError),
    StateSyncClientError::StateSyncError(StateSyncError::BlockNotFound(BlockNumber::default()))
)]
#[case::both_fail(
    BatcherClientError::BatcherError(BatcherError::InternalError),
    StateSyncClientError::ClientError(ClientError::CommunicationFailure("".to_string())),
)]
#[tokio::test]
async fn retrospective_block_hash_sad_flow(
    #[case] batcher_error: BatcherClientError,
    #[case] state_sync_error: StateSyncClientError,
) {
    let (mut test_proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    test_proposal_args.proposal_init.height = CURRENT_BLOCK_NUMBER;
    // Clone errors for use in match statement after closures capture the originals.
    let batcher_error_cloned = batcher_error.clone();
    let state_sync_error_cloned = state_sync_error.clone();
    // Setup batcher client to return an error.
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETTRO_BLOCK_NUMBER)
        .returning(move |_| Err(batcher_error.clone()));
    // Setup state sync client to return an error.
    test_proposal_args
        .deps
        .state_sync_client
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETTRO_BLOCK_NUMBER)
        .returning(move |_| Err(state_sync_error.clone()));

    let proposal_args: ProposalBuildArguments = test_proposal_args.into();
    let block_info = get_block_info(&proposal_args).await;
    let res = retrospective_block_hash(
        proposal_args.deps.batcher,
        proposal_args.deps.state_sync_client,
        &block_info,
    )
    .await;

    match (batcher_error_cloned, state_sync_error_cloned) {
        (BatcherClientError::BatcherError(BatcherError::BlockHashNotFound(_)), _)
        | (_, StateSyncClientError::StateSyncError(StateSyncError::BlockNotFound(_))) => {
            assert!(matches!(res, Err(RetrospectiveBlockHashError::NotReady { .. })));
        }
        (_, _) => {
            assert!(matches!(res, Err(RetrospectiveBlockHashError::FailedRetrievingHash { .. })));
        }
    }
}

#[tokio::test]
async fn wait_for_retrospective_block_hash_state_sync_ready_after_a_while() {
    let (mut test_proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    test_proposal_args.proposal_init.height = CURRENT_BLOCK_NUMBER;
    // Setup batcher client to return an error.
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETTRO_BLOCK_NUMBER)
        .returning(|_| Err(BatcherClientError::BatcherError(BatcherError::InternalError)));
    // Setup state sync client to return BlockNotFound error in the first attempt.
    test_proposal_args
        .deps
        .state_sync_client
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETTRO_BLOCK_NUMBER)
        .times(1)
        .returning(|_| Err(StateSyncError::BlockNotFound(BlockNumber::default()).into()));
    // Setup state sync client to return a block hash in the second attempt.
    test_proposal_args
        .deps
        .state_sync_client
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETTRO_BLOCK_NUMBER)
        .times(1)
        .returning(|_| Ok(BlockHash::default()));

    let proposal_args: ProposalBuildArguments = test_proposal_args.into();
    let block_info = get_block_info(&proposal_args).await;
    let res = wait_for_retrospective_block_hash(
        proposal_args.deps.batcher,
        proposal_args.deps.state_sync_client,
        &block_info,
        proposal_args.deps.clock.as_ref(),
        proposal_args.retrospective_block_hash_deadline,
        proposal_args.retrospective_block_hash_retry_interval_millis,
    )
    .await
    .unwrap();
    assert_eq!(
        res,
        Some(BlockHashAndNumber { number: RETTRO_BLOCK_NUMBER, hash: BlockHash::default() })
    );
}

#[tokio::test]
async fn wait_for_retrospective_block_hash_batcher_ready_after_a_while() {
    let (mut test_proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    test_proposal_args.proposal_init.height = CURRENT_BLOCK_NUMBER;
    // Setup batcher client to return BlockHashNotFound error in the first attempt.
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETTRO_BLOCK_NUMBER)
        .times(1)
        .returning(|_| {
            Err(BatcherClientError::BatcherError(BatcherError::BlockHashNotFound(
                BlockNumber::default(),
            )))
        });
    // Setup batcher client to return a block hash in the second attempt.
    test_proposal_args
        .deps
        .batcher
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETTRO_BLOCK_NUMBER)
        .times(1)
        .returning(|_| Ok(BlockHash::default()));
    // Setup state sync client to return BlockNotFound error.
    test_proposal_args
        .deps
        .state_sync_client
        .expect_get_block_hash()
        .withf(|block_number| *block_number == RETTRO_BLOCK_NUMBER)
        .returning(|_| Err(StateSyncError::BlockNotFound(BlockNumber::default()).into()));

    let proposal_args: ProposalBuildArguments = test_proposal_args.into();
    let block_info = get_block_info(&proposal_args).await;
    let res = wait_for_retrospective_block_hash(
        proposal_args.deps.batcher,
        proposal_args.deps.state_sync_client,
        &block_info,
        proposal_args.deps.clock.as_ref(),
        proposal_args.retrospective_block_hash_deadline,
        proposal_args.retrospective_block_hash_retry_interval_millis,
    )
    .await
    .unwrap();
    assert_eq!(
        res,
        Some(BlockHashAndNumber { number: RETTRO_BLOCK_NUMBER, hash: BlockHash::default() })
    );
}
