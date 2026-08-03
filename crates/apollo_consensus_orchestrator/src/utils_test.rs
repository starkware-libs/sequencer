use apollo_batcher_types::communication::BatcherClientError;
#[cfg(feature = "os_input")]
use apollo_batcher_types::communication::MockBatcherClient;
use apollo_batcher_types::errors::BatcherError;
use apollo_protobuf::consensus::ProposalInit;
use apollo_state_sync_types::communication::StateSyncClientError;
use apollo_state_sync_types::errors::StateSyncError;
use assert_matches::assert_matches;
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
#[cfg(feature = "os_input")]
use rstest::rstest;
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};
#[cfg(feature = "os_input")]
use starknet_committer::patricia_merkle_tree::types::CompressedStateCommitmentInfos;
use starknet_types_core::felt::Felt;

use crate::build_proposal::ProposalBuildArguments;
#[cfg(feature = "os_input")]
use crate::cende::MockCendeContext;
use crate::test_utils::create_proposal_build_arguments;
use crate::utils::{
    get_l1_prices_in_fri_and_wei,
    retrospective_block_hash,
    wait_for_retrospective_block_hash,
    RetrospectiveBlockHashError,
};
#[cfg(feature = "os_input")]
use crate::utils::{
    verify_retrospective_state_commitment_infos,
    RetrospectiveStateCommitmentInfosError,
};

const CURRENT_BLOCK_NUMBER: BlockNumber = BlockNumber(STORED_BLOCK_HASH_BUFFER);
const RETRO_BLOCK_NUMBER: BlockNumber = BlockNumber(0);
#[cfg(feature = "os_input")]
const NEXT_HEIGHT_RETRO_BLOCK_NUMBER: BlockNumber =
    BlockNumber(CURRENT_BLOCK_NUMBER.0 + 1 - STORED_BLOCK_HASH_BUFFER);
// A recorder offset above the next height's retrospective block number means its commitment infos
// are stored; an offset equal to it means they are missing.
#[cfg(feature = "os_input")]
const STORED_HEIGHT_OFFSET: Option<BlockNumber> =
    Some(BlockNumber(NEXT_HEIGHT_RETRO_BLOCK_NUMBER.0 + 1));
#[cfg(feature = "os_input")]
const BEHIND_HEIGHT_OFFSET: Option<BlockNumber> = Some(NEXT_HEIGHT_RETRO_BLOCK_NUMBER);
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

#[cfg(feature = "os_input")]
fn mock_batcher_commitment_infos(batcher_has_infos: bool) -> MockBatcherClient {
    let mut batcher = MockBatcherClient::new();
    batcher.expect_get_state_commitment_infos().times(1).returning(move |block_number| {
        assert_eq!(block_number, NEXT_HEIGHT_RETRO_BLOCK_NUMBER);
        Ok(batcher_has_infos
            .then(|| CompressedStateCommitmentInfos(b"compressed-state-commitment-infos".to_vec())))
    });
    batcher
}

#[cfg(feature = "os_input")]
fn mock_cende_recorder_height_offset(height_offset: Option<BlockNumber>) -> MockCendeContext {
    let mut cende_ambassador = MockCendeContext::new();
    cende_ambassador
        .expect_commitment_infos_height_offset()
        .times(1)
        .returning(move || Ok(height_offset));
    cende_ambassador
}

#[cfg(feature = "os_input")]
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

#[cfg(feature = "os_input")]
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
