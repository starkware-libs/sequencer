use std::cmp::min;
use std::collections::HashMap;

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
use papyrus_test_utils::get_rng;
use starknet_api::block::{BlockHeader, BlockHeaderWithoutHash, BlockNumber};
use starknet_api::core::{ascii_as_felt, ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::{StorageKey, ThinStateDiff};
use starknet_types_core::felt::Felt;
use tokio::sync::mpsc::{channel, Receiver};

use super::test_utils::{
    create_block_hashes_and_signatures,
    random_header,
    run_test,
    wait_for_marker,
    Action,
    DataType,
    HeaderTestPayload,
    StateDiffTestPayload,
    HEADER_QUERY_LENGTH,
    SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
    STATE_DIFF_QUERY_LENGTH,
    TIMEOUT_FOR_TEST,
    WAIT_PERIOD_FOR_NEW_DATA,
};
use super::{P2PSyncClientConfig, StateDiffQuery};

#[tokio::test]
async fn state_diff_basic_flow() {
    let mut rng = get_rng();

    let class_hash0 = ClassHash(ascii_as_felt("class_hash0").unwrap());
    let class_hash1 = ClassHash(ascii_as_felt("class_hash1").unwrap());
    let casm_hash0 = CompiledClassHash(ascii_as_felt("casm_hash0").unwrap());
    let address0 = ContractAddress(ascii_as_felt("address0").unwrap().try_into().unwrap());
    let address1 = ContractAddress(ascii_as_felt("address1").unwrap().try_into().unwrap());
    let address2 = ContractAddress(ascii_as_felt("address2").unwrap().try_into().unwrap());
    let key0 = StorageKey(ascii_as_felt("key0").unwrap().try_into().unwrap());
    let key1 = StorageKey(ascii_as_felt("key1").unwrap().try_into().unwrap());
    let value0 = ascii_as_felt("value0").unwrap();
    let value1 = ascii_as_felt("value1").unwrap();
    let nonce0 = Nonce(ascii_as_felt("nonce0").unwrap());

    let state_diffs_and_chunks = vec![
        (
            ThinStateDiff {
                deployed_contracts: indexmap!(address0 => class_hash0),
                storage_diffs: indexmap!(address0 => indexmap!(key0 => value0, key1 => value1)),
                declared_classes: indexmap!(class_hash0 => casm_hash0),
                deprecated_declared_classes: vec![class_hash1],
                nonces: indexmap!(address0 => nonce0),
                replaced_classes: Default::default(),
            },
            vec![
                StateDiffChunk::DeclaredClass(DeclaredClass {
                    class_hash: class_hash0,
                    compiled_class_hash: casm_hash0,
                }),
                StateDiffChunk::ContractDiff(ContractDiff {
                    contract_address: address0,
                    class_hash: Some(class_hash0),
                    nonce: Some(nonce0),
                    storage_diffs: indexmap!(key0 => value0, key1 => value1),
                }),
                StateDiffChunk::DeprecatedDeclaredClass(DeprecatedDeclaredClass {
                    class_hash: class_hash1,
                }),
            ],
        ),
        (
            ThinStateDiff {
                deployed_contracts: indexmap!(address1 => class_hash1),
                storage_diffs: indexmap!(
                    address1 => indexmap!(key0 => value0),
                    address2 => indexmap!(key1 => value1)
                ),
                nonces: indexmap!(address2 => nonce0),
                ..Default::default()
            },
            vec![
                StateDiffChunk::ContractDiff(ContractDiff {
                    contract_address: address1,
                    class_hash: Some(class_hash1),
                    nonce: None,
                    storage_diffs: indexmap!(key0 => value0),
                }),
                StateDiffChunk::ContractDiff(ContractDiff {
                    contract_address: address2,
                    class_hash: None,
                    nonce: Some(nonce0),
                    storage_diffs: indexmap!(key1 => value1),
                }),
            ],
        ),
    ];

    let mut actions = vec![
        // We already validate the header query content in other tests.
        Action::ReceiveQuery(Box::new(|_query| ()), DataType::Header),
    ];

    // Send headers with corresponding state diff length
    for (i, (state_diff, _)) in state_diffs_and_chunks.iter().enumerate() {
        actions.push(Action::SendHeader(DataOrFin(Some(random_header(
            &mut rng,
            BlockNumber(i.try_into().unwrap()),
            Some(state_diff.len()),
            None,
        )))));
    }
    actions.push(Action::SendHeader(DataOrFin(None)));

    let len = state_diffs_and_chunks.len();
    actions.push(Action::ReceiveQuery(
        Box::new(move |query| {
            assert_eq!(
                query,
                Query {
                    start_block: BlockHashOrNumber::Number(BlockNumber(0)),
                    direction: Direction::Forward,
                    limit: len.try_into().unwrap(),
                    step: 1,
                }
            )
        }),
        DataType::StateDiff,
    ));
    // Send state diff chunks and check storage
    for (i, (expected_state_diff, state_diff_chunks)) in
        state_diffs_and_chunks.iter().cloned().enumerate()
    {
        for state_diff_chunk in state_diff_chunks {
            // Check that before the last chunk was sent, the state diff isn't written.
            actions.push(Action::CheckStorage(Box::new(move |reader| {
                async move {
                    assert_eq!(
                        u64::try_from(i).unwrap(),
                        reader.begin_ro_txn().unwrap().get_state_marker().unwrap().0
                    );
                }
                .boxed()
            })));

            actions.push(Action::SendStateDiff(DataOrFin(Some(state_diff_chunk))));
        }
        // Check that a block's state diff is written before the entire query finished.
        actions.push(Action::CheckStorage(Box::new(move |reader| {
            async move {
                let block_number = BlockNumber(i.try_into().unwrap());
                wait_for_marker(
                    DataType::StateDiff,
                    &reader,
                    block_number.unchecked_next(),
                    SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
                    TIMEOUT_FOR_TEST,
                )
                .await;

                let txn = reader.begin_ro_txn().unwrap();
                let actual_state_diff = txn.get_state_diff(block_number).unwrap().unwrap();
                assert_eq!(actual_state_diff, expected_state_diff);
            }
            .boxed()
        })));
    }
    actions.push(Action::SendStateDiff(DataOrFin(None)));

    run_test(
        HashMap::from([
            (DataType::Header, state_diffs_and_chunks.len().try_into().unwrap()),
            (DataType::StateDiff, state_diffs_and_chunks.len().try_into().unwrap()),
        ]),
        actions,
    )
    .await;
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
    let mut rng = get_rng();

    let mut actions = vec![
        // We already validate the header query content in other tests.
        Action::ReceiveQuery(Box::new(|_query| ()), DataType::Header),
    ];

    // Send headers with corresponding state diff length
    for (i, state_diff_length) in header_state_diff_lengths.iter().copied().enumerate() {
        actions.push(Action::SendHeader(DataOrFin(Some(random_header(
            &mut rng,
            BlockNumber(i.try_into().unwrap()),
            Some(state_diff_length),
            None,
        )))));
    }
    actions.push(Action::SendHeader(DataOrFin(None)));

    actions.push(
        // We already validate the state diff query content in other tests.
        Action::ReceiveQuery(Box::new(|_query| ()), DataType::StateDiff),
    );

    // Send state diff chunks.
    for state_diff_chunk in state_diff_chunks {
        actions.push(Action::SendStateDiff(DataOrFin(state_diff_chunk)));
    }

    actions.push(Action::ValidateReportSent(DataType::StateDiff));

    run_test(
        HashMap::from([
            (DataType::Header, header_state_diff_lengths.len().try_into().unwrap()),
            (DataType::StateDiff, header_state_diff_lengths.len().try_into().unwrap()),
        ]),
        actions,
    )
    .await;
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
    for headers_for_current_query in block_hashes_and_signatures
        .into_iter()
        .zip(header_state_diff_lengths.clone().into_iter())
        .enumerate()
        .collect::<Vec<_>>()
        .chunks(HEADER_QUERY_LENGTH.try_into().unwrap())
        .map(Vec::from)
    {
        // Receive the next query from header sync
        let mut mock_header_responses_manager = mock_header_response_manager.next().await.unwrap();

        for (i, ((block_hash, block_signature), header_state_diff_length)) in
            headers_for_current_query
        {
            // Send header responses
            mock_header_responses_manager
                .send_response(DataOrFin(Some(SignedBlockHeader {
                    block_header: BlockHeader {
                        block_hash,
                        block_header_without_hash: BlockHeaderWithoutHash {
                            block_number: BlockNumber(u64::try_from(i).unwrap()),
                            ..Default::default()
                        },
                        state_diff_length: Some(header_state_diff_length),
                        ..Default::default()
                    },
                    signatures: vec![block_signature],
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
