use apollo_batcher_types::communication::BatcherClientError;
use apollo_batcher_types::errors::BatcherError;
use apollo_protobuf::consensus::ProposalInit;
use apollo_state_sync_types::communication::StateSyncClientError;
use apollo_state_sync_types::errors::StateSyncError;
use assert_matches::assert_matches;
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber, GasPrice};
use starknet_types_core::felt::Felt;

use crate::build_proposal::ProposalBuildArguments;
use crate::test_utils::create_proposal_build_arguments;
use crate::utils::{
    get_l1_prices_in_fri_and_wei,
    retrospective_block_hash,
    wait_for_retrospective_block_hash,
    RetrospectiveBlockHashError,
};

const CURRENT_BLOCK_NUMBER: BlockNumber = BlockNumber(STORED_BLOCK_HASH_BUFFER);
const RETRO_BLOCK_NUMBER: BlockNumber = BlockNumber(0);
const MUST_HAVE_BLOCK_HASH_FOR: BlockNumber = BlockNumber(1);
const RETRO_BLOCK_HASH: BlockHash = BlockHash(Felt::from_hex_unchecked("0x1234567890abcdef"));

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
        fee_proposal: GasPrice::default(),
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
