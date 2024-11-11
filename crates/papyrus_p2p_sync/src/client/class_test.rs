use std::time::Duration;

use futures::{FutureExt, StreamExt};
use papyrus_common::pending_classes::ApiContractClass;
use papyrus_protobuf::sync::{
    BlockHashOrNumber,
    ClassQuery,
    DataOrFin,
    DeclaredClass,
    DeprecatedDeclaredClass,
    Direction,
    Query,
    SignedBlockHeader,
    StateDiffChunk,
};
use papyrus_storage::class::ClassStorageReader;
use papyrus_storage::state::StateStorageReader;
use papyrus_test_utils::{get_rng, GetTestInstance};
use rand::RngCore;
use rand_chacha::ChaCha8Rng;
use starknet_api::block::{BlockHeader, BlockHeaderWithoutHash, BlockNumber};
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::state::ContractClass;
use static_assertions::const_assert;

use super::test_utils::{
    create_block_hashes_and_signatures,
    setup,
    TestArgs,
    HEADER_QUERY_LENGTH,
    SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
    STATE_DIFF_QUERY_LENGTH,
    WAIT_PERIOD_FOR_NEW_DATA,
};

const TIMEOUT_FOR_TEST: Duration = Duration::from_secs(5);

#[tokio::test]
async fn class_basic_flow() {
    // Asserting the constants so the test can assume there will be 2 state diff queries for a
    // single header query and the second will be smaller than the first.
    const_assert!(STATE_DIFF_QUERY_LENGTH < HEADER_QUERY_LENGTH);
    const_assert!(HEADER_QUERY_LENGTH < 2 * STATE_DIFF_QUERY_LENGTH);

    let TestArgs {
        p2p_sync,
        storage_reader,
        mut mock_state_diff_response_manager,
        mut mock_header_response_manager,
        mut mock_class_response_manager,
        // The test will fail if we drop this
        mock_transaction_response_manager: _mock_transaction_responses_manager,
        ..
    } = setup();

    let block_hashes_and_signatures =
        create_block_hashes_and_signatures(HEADER_QUERY_LENGTH.try_into().unwrap());
    let mut rng = get_rng();
    let state_diffs = (0..HEADER_QUERY_LENGTH)
        .map(|_| create_random_state_diff_chunk(&mut rng))
        .collect::<Vec<_>>();

    // Create a future that will receive queries, send responses and validate the results
    let parse_queries_future = async move {
        // We wait for the state diff sync to see that there are no headers and start sleeping
        tokio::time::sleep(SLEEP_DURATION_TO_LET_SYNC_ADVANCE).await;

        // Check that before we send headers there is no state diff query.
        assert!(mock_state_diff_response_manager.next().now_or_never().is_none());
        let mut mock_header_responses_manager = mock_header_response_manager.next().await.unwrap();

        // Send headers for entire query
        for (i, ((block_hash, block_signature), state_diff)) in
            block_hashes_and_signatures.iter().zip(state_diffs.iter()).enumerate()
        {
            // Send responses
            mock_header_responses_manager
                .send_response(DataOrFin(Some(SignedBlockHeader {
                    block_header: BlockHeader {
                        block_hash: *block_hash,
                        block_header_without_hash: BlockHeaderWithoutHash {
                            block_number: BlockNumber(i.try_into().unwrap()),
                            ..Default::default()
                        },
                        state_diff_length: Some(state_diff.len()),
                        ..Default::default()
                    },
                    signatures: vec![*block_signature],
                })))
                .await
                .unwrap();
        }

        // We wait for the header sync to write the new headers
        tokio::time::sleep(SLEEP_DURATION_TO_LET_SYNC_ADVANCE).await;

        // Simulate time has passed so that state diff sync will resend query after it waited for
        // new header
        tokio::time::pause();
        tokio::time::advance(WAIT_PERIOD_FOR_NEW_DATA).await;
        tokio::time::resume();

        for (start_block_number, num_blocks) in [
            (0u64, STATE_DIFF_QUERY_LENGTH),
            (STATE_DIFF_QUERY_LENGTH, HEADER_QUERY_LENGTH - STATE_DIFF_QUERY_LENGTH),
        ] {
            // Check that before we send state diffs there is no class query.
            assert!(mock_class_response_manager.next().now_or_never().is_none());

            let mut mock_state_diff_responses_manager =
                mock_state_diff_response_manager.next().await.unwrap();

            let mut classes = Vec::new();
            for block_number in start_block_number..(start_block_number + num_blocks) {
                let state_diff_chunk = state_diffs[usize::try_from(block_number).unwrap()].clone();

                let block_number = BlockNumber(block_number);

                // Check that before we've sent all parts the state diff wasn't written yet
                let txn = storage_reader.begin_ro_txn().unwrap();
                assert_eq!(block_number, txn.get_state_marker().unwrap());

                mock_state_diff_responses_manager
                    .send_response(DataOrFin(Some(state_diff_chunk.clone())))
                    .await
                    .unwrap();

                classes.push(create_random_class(state_diff_chunk.clone(), &mut rng));
                tokio::time::sleep(SLEEP_DURATION_TO_LET_SYNC_ADVANCE).await;
            }
            mock_state_diff_responses_manager.send_response(DataOrFin(None)).await.unwrap();

            let mut mock_class_responses_manager =
                mock_class_response_manager.next().await.unwrap();
            assert_eq!(
                *mock_class_responses_manager.query(),
                Ok(ClassQuery(Query {
                    start_block: BlockHashOrNumber::Number(BlockNumber(start_block_number)),
                    direction: Direction::Forward,
                    limit: num_blocks,
                    step: 1,
                })),
                "If the limit of the query is too low, try to increase \
                 SLEEP_DURATION_TO_LET_SYNC_ADVANCE",
            );

            let mut block_number = BlockNumber(start_block_number);
            for (class, class_hash) in classes.clone() {
                // Check that before we've sent all parts the class wasn't written yet
                let txn = storage_reader.begin_ro_txn().unwrap();
                assert_eq!(block_number, txn.get_class_marker().unwrap());

                mock_class_responses_manager
                    .send_response(DataOrFin(Some((class.clone(), class_hash))))
                    .await
                    .unwrap();

                tokio::time::sleep(SLEEP_DURATION_TO_LET_SYNC_ADVANCE).await;

                // Check that class was written to the storage. This way we make sure that the sync
                // writes to the storage each block's classes before receiving all query
                // responses.
                let txn = storage_reader.begin_ro_txn().unwrap();
                block_number = block_number.unchecked_next();
                assert_eq!(block_number, txn.get_class_marker().unwrap());

                let expected_class = match class {
                    ApiContractClass::ContractClass(_) => ApiContractClass::ContractClass(
                        txn.get_class(&class_hash).unwrap().unwrap(),
                    ),
                    ApiContractClass::DeprecatedContractClass(_) => {
                        ApiContractClass::DeprecatedContractClass(
                            txn.get_deprecated_class(&class_hash).unwrap().unwrap(),
                        )
                    }
                };
                assert_eq!(class, expected_class);
            }

            mock_class_responses_manager.send_response(DataOrFin(None)).await.unwrap();

            tokio::time::sleep(SLEEP_DURATION_TO_LET_SYNC_ADVANCE).await;
        }
    };

    tokio::select! {
        sync_result = p2p_sync.run() => {
            sync_result.unwrap();
            panic!("P2P sync aborted with no failure.");
        }
        _ = parse_queries_future => {}
    }
}

fn create_random_state_diff_chunk(rng: &mut ChaCha8Rng) -> StateDiffChunk {
    let class_hash = ClassHash(rng.next_u64().into());
    if rng.next_u32() % 2 == 0 {
        StateDiffChunk::DeclaredClass(DeclaredClass {
            class_hash,
            compiled_class_hash: CompiledClassHash(rng.next_u64().into()),
        })
    } else {
        StateDiffChunk::DeprecatedDeclaredClass(DeprecatedDeclaredClass { class_hash })
    }
}

fn create_random_class(
    state_diff_chunk: StateDiffChunk,
    rng: &mut ChaCha8Rng,
) -> (ApiContractClass, ClassHash) {
    match state_diff_chunk {
        StateDiffChunk::DeclaredClass(declared_class) => (
            ApiContractClass::ContractClass(ContractClass::get_test_instance(rng)),
            declared_class.class_hash,
        ),
        StateDiffChunk::DeprecatedDeclaredClass(deprecated_declared_class) => (
            ApiContractClass::DeprecatedContractClass(DeprecatedContractClass::get_test_instance(
                rng,
            )),
            deprecated_declared_class.class_hash,
        ),
        _ => unreachable!(),
    }
}

// TODO(noamsp): Consider verifying that ParseDataError::BadPeerError(NotEnoughClasses)
// was returned from parse_data_for_block. We currently dont have a way to check this.
#[tokio::test]
async fn not_enough_classes() {
    validate_class_sync_fails(
        2,
        vec![
            Some(StateDiffChunk::DeclaredClass(DeclaredClass::default())),
            Some(StateDiffChunk::DeprecatedDeclaredClass(DeprecatedDeclaredClass::default())),
        ],
        vec![
            Some((
                ApiContractClass::ContractClass(ContractClass::get_test_instance(&mut get_rng())),
                ClassHash::default(),
            )),
            None,
        ],
    )
    .await;
}

// TODO(noamsp): Consider verifying that ParseDataError::BadPeerError(ClassNotInStateDiff)
// was returned from parse_data_for_block. We currently dont have a way to check this.
#[tokio::test]
async fn class_not_in_state_diff() {
    validate_class_sync_fails(
        1,
        vec![Some(StateDiffChunk::DeclaredClass(DeclaredClass::default()))],
        vec![Some((
            ApiContractClass::DeprecatedContractClass(DeprecatedContractClass::get_test_instance(
                &mut get_rng(),
            )),
            ClassHash::default(),
        ))],
    )
    .await;
}

// TODO(noamsp): Consider verifying that ParseDataError::BadPeerError(DuplicateClass)
// was returned from parse_data_for_block. We currently dont have a way to check this.
#[tokio::test]
async fn duplicate_classes() {
    validate_class_sync_fails(
        2,
        vec![
            Some(StateDiffChunk::DeclaredClass(DeclaredClass::default())),
            Some(StateDiffChunk::DeprecatedDeclaredClass(DeprecatedDeclaredClass::default())),
        ],
        vec![
            Some((
                ApiContractClass::DeprecatedContractClass(
                    DeprecatedContractClass::get_test_instance(&mut get_rng()),
                ),
                ClassHash::default(),
            )),
            Some((
                ApiContractClass::DeprecatedContractClass(
                    DeprecatedContractClass::get_test_instance(&mut get_rng()),
                ),
                ClassHash::default(),
            )),
        ],
    )
    .await;
}

async fn validate_class_sync_fails(
    state_diff_length_in_header: usize,
    state_diff_chunks: Vec<Option<StateDiffChunk>>,
    classes: Vec<Option<(ApiContractClass, ClassHash)>>,
) {
    let TestArgs {
        p2p_sync,
        storage_reader,
        mut mock_state_diff_response_manager,
        mut mock_header_response_manager,
        mut mock_class_response_manager,
        // The test will fail if we drop this
        mock_transaction_response_manager: _mock_transaction_responses_manager,
        ..
    } = setup();

    let (block_hash, block_signature) = *create_block_hashes_and_signatures(1).first().unwrap();

    // Create a future that will receive queries, send responses and validate the results.
    let parse_queries_future = async move {
        // Send a single header. There's no need to fill the entire query.
        let mut mock_header_responses_manager = mock_header_response_manager.next().await.unwrap();
        mock_header_responses_manager
            .send_response(DataOrFin(Some(SignedBlockHeader {
                block_header: BlockHeader {
                    block_hash,
                    block_header_without_hash: BlockHeaderWithoutHash {
                        block_number: BlockNumber(0),
                        ..Default::default()
                    },
                    state_diff_length: Some(state_diff_length_in_header),
                    ..Default::default()
                },
                signatures: vec![block_signature],
            })))
            .await
            .unwrap();

        // We wait for the header sync to write the new headers.
        tokio::time::sleep(SLEEP_DURATION_TO_LET_SYNC_ADVANCE).await;

        // Simulate time has passed so that state diff sync will resend query after it waited for
        // new header
        tokio::time::pause();
        tokio::time::advance(WAIT_PERIOD_FOR_NEW_DATA).await;
        tokio::time::resume();

        let mut mock_state_diff_responses_manager =
            mock_state_diff_response_manager.next().await.unwrap();

        // Send state diffs.
        for state_diff_chunk in state_diff_chunks {
            // Check that before we've sent all parts the state diff wasn't written yet.
            let txn = storage_reader.begin_ro_txn().unwrap();
            assert_eq!(0, txn.get_state_marker().unwrap().0);

            mock_state_diff_responses_manager
                .send_response(DataOrFin(state_diff_chunk))
                .await
                .unwrap();
        }

        tokio::time::sleep(SLEEP_DURATION_TO_LET_SYNC_ADVANCE).await;

        let mut mock_classes_responses_manager = mock_class_response_manager.next().await.unwrap();
        assert_eq!(
            *mock_classes_responses_manager.query(),
            Ok(ClassQuery(Query {
                start_block: BlockHashOrNumber::Number(BlockNumber(0)),
                direction: Direction::Forward,
                limit: 1,
                step: 1,
            }))
        );

        for class in classes {
            // Check that before we've sent all parts the state diff wasn't written yet.
            let txn = storage_reader.begin_ro_txn().unwrap();
            assert_eq!(0, txn.get_class_marker().unwrap().0);

            mock_classes_responses_manager.send_response(DataOrFin(class)).await.unwrap();
        }

        // Asserts that a peer was reported due to a non-fatal error.
        mock_classes_responses_manager.assert_reported(TIMEOUT_FOR_TEST).await;
    };

    tokio::select! {
        sync_result = p2p_sync.run() => {
            sync_result.unwrap();
            panic!("P2P sync aborted with no failure.");
        }
        _ = parse_queries_future => {}
    }
}
