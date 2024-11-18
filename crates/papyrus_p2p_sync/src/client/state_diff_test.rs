use std::cmp::min;

use futures::future::join;
use futures::{FutureExt, StreamExt};
use indexmap::indexmap;
use papyrus_network::network_manager::GenericReceiver;
use papyrus_protobuf::sync::{
    BlockHashOrNumber,
    ContractDiff,
    DataOrFin,
    DeclaredClass,
    DeprecatedDeclaredClass,
    Direction,
    Query,
    SignedBlockHeader,
    StateDiffChunk,
};
use papyrus_storage::state::StateStorageReader;
use papyrus_test_utils::{get_rng, GetTestInstance};
use rand::RngCore;
use rand_chacha::ChaCha8Rng;
use starknet_api::block::{BlockHeader, BlockHeaderWithoutHash, BlockNumber};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::{StorageKey, ThinStateDiff};
use starknet_types_core::felt::Felt;
use static_assertions::const_assert;
use tokio::sync::mpsc::{channel, Receiver};

use super::test_utils::{
    create_block_hashes_and_signatures,
    setup,
    wait_for_marker,
    HeaderTestPayload,
    MarkerKind,
    StateDiffTestPayload,
    TestArgs,
    HEADER_QUERY_LENGTH,
    SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
    STATE_DIFF_QUERY_LENGTH,
    TIMEOUT_FOR_TEST,
    WAIT_PERIOD_FOR_NEW_DATA,
};
use super::{P2PSyncClientConfig, StateDiffQuery};

#[tokio::test]
async fn state_diff_basic_flow() {
    // Asserting the constants so the test can assume there will be 2 state diff queries for a
    // single header query and the second will be smaller than the first.
    const_assert!(STATE_DIFF_QUERY_LENGTH < HEADER_QUERY_LENGTH);
    const_assert!(HEADER_QUERY_LENGTH < 2 * STATE_DIFF_QUERY_LENGTH);

    let TestArgs {
        p2p_sync,
        storage_reader,
        mut mock_state_diff_response_manager,
        mut mock_header_response_manager,
        // The test will fail if we drop these
        mock_transaction_response_manager: _mock_transaction_responses_manager,
        mock_class_response_manager: _mock_class_responses_manager,
        ..
    } = setup();

    let mut rng = get_rng();
    // TODO(eitan): Add a 3rd constant for NUM_CHUNKS_PER_BLOCK so that ThinStateDiff is made from
    // multiple StateDiffChunks
    let (state_diffs, header_state_diff_lengths): (Vec<_>, Vec<_>) = (0..HEADER_QUERY_LENGTH)
        .map(|_| {
            let diff = create_random_state_diff_chunk(&mut rng);
            let length = diff.len();
            (diff, length)
        })
        .unzip();

    let (state_diff_sender, mut state_diff_receiver) = channel(p2p_sync.config.buffer_size);

    // Create a future that will receive send responses and validate the results.
    let test_future = async move {
        for (start_block_number, num_blocks) in [
            (0u64, STATE_DIFF_QUERY_LENGTH),
            (STATE_DIFF_QUERY_LENGTH, HEADER_QUERY_LENGTH - STATE_DIFF_QUERY_LENGTH),
        ] {
            for block_number in start_block_number..(start_block_number + num_blocks) {
                let state_diff_chunk = state_diffs[usize::try_from(block_number).unwrap()].clone();

                let block_number = BlockNumber(block_number);

                // Check that before we've sent all parts the state diff wasn't written yet.
                let txn = storage_reader.begin_ro_txn().unwrap();
                assert_eq!(block_number, txn.get_state_marker().unwrap());

                state_diff_sender.send(Some(state_diff_chunk.clone())).await.unwrap();

                // Check state diff was written to the storage. This way we make sure that the sync
                // writes to the storage each block's state diff before receiving all query
                // responses.

                wait_for_marker(
                    MarkerKind::State,
                    &storage_reader,
                    block_number.unchecked_next(),
                    SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
                    TIMEOUT_FOR_TEST,
                )
                .await;

                let txn = storage_reader.begin_ro_txn().unwrap();
                let state_diff = txn.get_state_diff(block_number).unwrap().unwrap();
                // TODO(noamsp): refactor test so that we treat multiple state diff chunks as a
                // single state diff
                let expected_state_diff = match state_diff_chunk {
                    StateDiffChunk::ContractDiff(contract_diff) => {
                        let mut deployed_contracts = indexmap! {};
                        if let Some(class_hash) = contract_diff.class_hash {
                            deployed_contracts.insert(contract_diff.contract_address, class_hash);
                        };
                        let mut nonces = indexmap! {};
                        if let Some(nonce) = contract_diff.nonce {
                            nonces.insert(contract_diff.contract_address, nonce);
                        }
                        ThinStateDiff {
                            deployed_contracts,
                            nonces,
                            storage_diffs: indexmap! {
                                contract_diff.contract_address => contract_diff.storage_diffs
                            },
                            ..Default::default()
                        }
                    }
                    StateDiffChunk::DeclaredClass(declared_class) => ThinStateDiff {
                        declared_classes: indexmap! {
                            declared_class.class_hash => declared_class.compiled_class_hash
                        },
                        ..Default::default()
                    },
                    StateDiffChunk::DeprecatedDeclaredClass(deprecated_declared_class) => {
                        ThinStateDiff {
                            deprecated_declared_classes: vec![deprecated_declared_class.class_hash],
                            ..Default::default()
                        }
                    }
                };
                assert_eq!(state_diff, expected_state_diff);
            }

            state_diff_sender.send(None).await.unwrap();
        }
    };

    tokio::select! {
        sync_result = p2p_sync.run() => {
            sync_result.unwrap();
            panic!("P2P sync aborted with no failure.");
        }
        _ = join(
                run_state_diff_sync_through_channel(
                    &mut mock_header_response_manager,
                    &mut mock_state_diff_response_manager,
                    header_state_diff_lengths,
                    &mut state_diff_receiver,
                    false,
                ),
                test_future,
            ) => {}
    }
}

// TODO(noamsp): Consider verifying that ParseDataError::BadPeerError(EmptyStateDiffPart) was
// returned from parse_data_for_block. We currently dont have a way to check this.
#[tokio::test]
async fn state_diff_empty_state_diff() {
    validate_state_diff_fails(vec![1], vec![Some(StateDiffChunk::default())]).await;
}

// TODO(noamsp): Consider verifying that ParseDataError::BadPeerError(WrongStateDiffLength) was
// returned from parse_data_for_block. We currently dont have a way to check this.
#[tokio::test]
async fn state_diff_stopped_in_middle() {
    validate_state_diff_fails(
        vec![2],
        vec![
            Some(StateDiffChunk::DeprecatedDeclaredClass(DeprecatedDeclaredClass::default())),
            None,
        ],
    )
    .await;
}

// TODO(noamsp): Consider verifying that ParseDataError::BadPeerError(WrongStateDiffLength) was
// returned from parse_data_for_block. We currently dont have a way to check this.
#[tokio::test]
async fn state_diff_not_split_correctly() {
    validate_state_diff_fails(
        vec![2],
        vec![
            Some(StateDiffChunk::DeprecatedDeclaredClass(DeprecatedDeclaredClass::default())),
            Some(StateDiffChunk::ContractDiff(ContractDiff {
                contract_address: ContractAddress::default(),
                class_hash: Some(ClassHash::default()),
                nonce: Some(Nonce::default()),
                ..Default::default()
            })),
        ],
    )
    .await;
}

// TODO(noamsp): Consider verifying that ParseDataError::BadPeerError(ConflictingStateDiffParts)
// was returned from parse_data_for_block. We currently dont have a way to check this.
#[tokio::test]
async fn state_diff_conflicting() {
    validate_state_diff_fails(
        vec![2],
        vec![
            Some(StateDiffChunk::ContractDiff(ContractDiff {
                contract_address: ContractAddress::default(),
                class_hash: Some(ClassHash::default()),
                ..Default::default()
            })),
            Some(StateDiffChunk::ContractDiff(ContractDiff {
                contract_address: ContractAddress::default(),
                class_hash: Some(ClassHash::default()),
                ..Default::default()
            })),
        ],
    )
    .await;
    validate_state_diff_fails(
        vec![2],
        vec![
            Some(StateDiffChunk::ContractDiff(ContractDiff {
                contract_address: ContractAddress::default(),
                storage_diffs: indexmap! { StorageKey::default() => Felt::default() },
                ..Default::default()
            })),
            Some(StateDiffChunk::ContractDiff(ContractDiff {
                contract_address: ContractAddress::default(),
                storage_diffs: indexmap! { StorageKey::default() => Felt::default() },
                ..Default::default()
            })),
        ],
    )
    .await;
    validate_state_diff_fails(
        vec![2],
        vec![
            Some(StateDiffChunk::DeclaredClass(DeclaredClass {
                class_hash: ClassHash::default(),
                compiled_class_hash: CompiledClassHash::default(),
            })),
            Some(StateDiffChunk::DeclaredClass(DeclaredClass {
                class_hash: ClassHash::default(),
                compiled_class_hash: CompiledClassHash::default(),
            })),
        ],
    )
    .await;
    validate_state_diff_fails(
        vec![2],
        vec![
            Some(StateDiffChunk::DeprecatedDeclaredClass(DeprecatedDeclaredClass {
                class_hash: ClassHash::default(),
            })),
            Some(StateDiffChunk::DeprecatedDeclaredClass(DeprecatedDeclaredClass {
                class_hash: ClassHash::default(),
            })),
        ],
    )
    .await;
    validate_state_diff_fails(
        vec![2],
        vec![
            Some(StateDiffChunk::ContractDiff(ContractDiff {
                contract_address: ContractAddress::default(),
                nonce: Some(Nonce::default()),
                ..Default::default()
            })),
            Some(StateDiffChunk::ContractDiff(ContractDiff {
                contract_address: ContractAddress::default(),
                nonce: Some(Nonce::default()),
                ..Default::default()
            })),
        ],
    )
    .await;
}

async fn validate_state_diff_fails(
    header_state_diff_lengths: Vec<usize>,
    state_diff_chunks: Vec<Option<StateDiffChunk>>,
) {
    let TestArgs {
        storage_reader,
        p2p_sync,
        mut mock_state_diff_response_manager,
        mut mock_header_response_manager,
        // The test will fail if we drop these
        mock_transaction_response_manager: _mock_transaction_responses_manager,
        mock_class_response_manager: _mock_class_responses_manager,
        ..
    } = setup();

    let (state_diff_sender, mut state_diff_receiver) = channel(p2p_sync.config.buffer_size);

    // Create a future that will send responses and validate the results.
    let test_future = async move {
        for state_diff_chunk in state_diff_chunks {
            // Check that before we've sent all parts the state diff wasn't written yet.
            let txn = storage_reader.begin_ro_txn().unwrap();
            assert_eq!(0, txn.get_state_marker().unwrap().0);

            state_diff_sender.send(state_diff_chunk).await.unwrap();
        }
    };

    tokio::select! {
        sync_result = p2p_sync.run() => {
            sync_result.unwrap();
            panic!("P2P sync aborted with no failure.");
        }
        _ = join(
            run_state_diff_sync_through_channel(
                &mut mock_header_response_manager,
                &mut mock_state_diff_response_manager,
                header_state_diff_lengths,
                &mut state_diff_receiver,
                true,
            ),
            test_future
         ) => {}
    }
}

// Advances the header sync with associated header state diffs.
// The receiver waits for external sender to provide the state diff chunks.
async fn run_state_diff_sync_through_channel(
    mock_header_response_manager: &mut GenericReceiver<HeaderTestPayload>,
    mock_state_diff_response_manager: &mut GenericReceiver<StateDiffTestPayload>,
    header_state_diff_lengths: Vec<usize>,
    state_diff_chunk_receiver: &mut Receiver<Option<StateDiffChunk>>,
    should_assert_reported: bool,
) {
    // We wait for the state diff sync to see that there are no headers and start sleeping
    tokio::time::sleep(SLEEP_DURATION_TO_LET_SYNC_ADVANCE).await;

    // Check that before we send headers there is no state diff query.
    assert!(mock_state_diff_response_manager.next().now_or_never().is_none());

    let num_headers = header_state_diff_lengths.len();
    let block_hashes_and_signatures =
        create_block_hashes_and_signatures(num_headers.try_into().unwrap());

    // split the headers into queries of size HEADER_QUERY_LENGTH and send headers for each query
    for header_querie in block_hashes_and_signatures
        .iter()
        .zip(header_state_diff_lengths.iter())
        .enumerate()
        .collect::<Vec<_>>()
        .chunks(HEADER_QUERY_LENGTH.try_into().unwrap())
    {
        let mut mock_header_responses_manager = mock_header_response_manager.next().await.unwrap();

        for (i, ((block_hash, block_signature), header_state_diff_length)) in header_querie {
            // Send header responses
            mock_header_responses_manager
                .send_response(DataOrFin(Some(SignedBlockHeader {
                    block_header: BlockHeader {
                        block_hash: *block_hash,
                        block_header_without_hash: BlockHeaderWithoutHash {
                            block_number: BlockNumber(u64::try_from(*i).unwrap()),
                            ..Default::default()
                        },
                        state_diff_length: Some(**header_state_diff_length),
                        ..Default::default()
                    },
                    signatures: vec![*block_signature],
                })))
                .await
                .unwrap();
        }

        mock_header_responses_manager.send_response(DataOrFin(None)).await.unwrap();
    }

    // TODO(noamsp): remove sleep and wait until header marker writes the new headers. remove the
    // comment from the StateDiffQuery about the limit being too low. We wait for the header
    // sync to write the new headers.
    tokio::time::sleep(SLEEP_DURATION_TO_LET_SYNC_ADVANCE).await;

    // Simulate time has passed so that state diff sync will resend query after it waited for
    // new header
    tokio::time::pause();
    tokio::time::advance(WAIT_PERIOD_FOR_NEW_DATA).await;
    tokio::time::resume();

    let num_state_diff_headers = u64::try_from(num_headers).unwrap();
    let num_state_diff_queries = num_state_diff_headers.div_ceil(STATE_DIFF_QUERY_LENGTH);

    for i in 0..num_state_diff_queries {
        let start_block_number = i * STATE_DIFF_QUERY_LENGTH;
        let limit = min(num_state_diff_headers - start_block_number, STATE_DIFF_QUERY_LENGTH);

        // Get a state diff query and validate it
        let mut mock_state_diff_responses_manager =
            mock_state_diff_response_manager.next().await.unwrap();
        assert_eq!(
            *mock_state_diff_responses_manager.query(),
            Ok(StateDiffQuery(Query {
                start_block: BlockHashOrNumber::Number(BlockNumber(start_block_number)),
                direction: Direction::Forward,
                limit,
                step: 1,
            })),
            "If the limit of the query is too low, try to increase \
             SLEEP_DURATION_TO_LET_SYNC_ADVANCE",
        );

        let mut current_state_diff_length = 0;
        let destination_state_diff_length =
            header_state_diff_lengths[start_block_number.try_into().unwrap()
                ..(start_block_number + limit).try_into().unwrap()]
                .iter()
                .sum();

        while current_state_diff_length < destination_state_diff_length {
            let state_diff_chunk = state_diff_chunk_receiver.recv().await.unwrap();

            mock_state_diff_responses_manager
                .send_response(DataOrFin(state_diff_chunk.clone()))
                .await
                .unwrap();

            if let Some(state_diff_chunk) = state_diff_chunk {
                if !state_diff_chunk.is_empty() {
                    current_state_diff_length += state_diff_chunk.len();
                    continue;
                }
            }

            break;
        }

        if should_assert_reported {
            mock_state_diff_responses_manager.assert_reported(TIMEOUT_FOR_TEST).await;
            continue;
        }

        assert_eq!(current_state_diff_length, destination_state_diff_length);
        let state_diff_chunk = state_diff_chunk_receiver.recv().await.unwrap();
        mock_state_diff_responses_manager
            .send_response(DataOrFin(state_diff_chunk.clone()))
            .await
            .unwrap();
    }
}

pub(crate) async fn run_state_diff_sync(
    config: P2PSyncClientConfig,
    mock_header_response_manager: &mut GenericReceiver<HeaderTestPayload>,
    mock_state_diff_response_manager: &mut GenericReceiver<StateDiffTestPayload>,
    header_state_diff_lengths: Vec<usize>,
    state_diff_chunks: Vec<Option<StateDiffChunk>>,
) {
    let (state_diff_sender, mut state_diff_receiver) = channel(config.buffer_size);
    tokio::join! {
        run_state_diff_sync_through_channel(
            mock_header_response_manager,
            mock_state_diff_response_manager,
            header_state_diff_lengths,
            &mut state_diff_receiver,
            false,
        ),
        async {
            for state_diff in state_diff_chunks.chunks(STATE_DIFF_QUERY_LENGTH.try_into().unwrap()) {
                for state_diff_chunk in state_diff {
                    state_diff_sender.send(state_diff_chunk.clone()).await.unwrap();
                }

                state_diff_sender.send(None).await.unwrap();
            }
        }
    };
}

fn create_random_state_diff_chunk(rng: &mut ChaCha8Rng) -> StateDiffChunk {
    let mut state_diff_chunk = StateDiffChunk::get_test_instance(rng);
    let contract_address = ContractAddress::from(rng.next_u64());
    let class_hash = ClassHash(rng.next_u64().into());
    match &mut state_diff_chunk {
        StateDiffChunk::ContractDiff(contract_diff) => {
            contract_diff.contract_address = contract_address;
            contract_diff.class_hash = Some(class_hash);
        }
        StateDiffChunk::DeclaredClass(declared_class) => {
            declared_class.class_hash = class_hash;
            declared_class.compiled_class_hash = CompiledClassHash(rng.next_u64().into());
        }
        StateDiffChunk::DeprecatedDeclaredClass(deprecated_declared_class) => {
            deprecated_declared_class.class_hash = class_hash;
        }
    }
    state_diff_chunk
}
