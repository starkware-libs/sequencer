use std::collections::HashMap;

use apollo_protobuf::sync::{
    BlockHashOrNumber,
    ContractDiff,
    DataOrFin,
    DeclaredClass,
    DeprecatedDeclaredClass,
    Direction,
    Query,
    StateDiffChunk,
};
use apollo_storage::state::StateStorageReader;
use apollo_test_utils::get_rng;
use futures::FutureExt;
use indexmap::indexmap;
use starknet_api::block::BlockNumber;
use starknet_api::compiled_class_hash;
use starknet_api::core::{ascii_as_felt, ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::state::{StorageKey, ThinStateDiff};
use starknet_types_core::felt::Felt;

use super::test_utils::{
    random_header,
    run_test,
    wait_for_marker,
    Action,
    DataType,
    SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
    TIMEOUT_FOR_TEST,
};

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
        Action::RunP2pSync,
        // We already validate the header query content in other tests.
        Action::ReceiveQuery(Box::new(|_query| ()), DataType::Header),
    ];

    // Sleep so state diff sync will reach the sleep waiting for header protocol to receive new
    // data.
    actions.push(Action::SleepToLetSyncAdvance);
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
    // Wait for header sync to finish before continuing state diff sync.
    actions.push(Action::CheckStorage(Box::new(move |reader| {
        async move {
            let block_number = BlockNumber(len.try_into().unwrap());
            wait_for_marker(
                DataType::Header,
                &reader,
                block_number,
                SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
                TIMEOUT_FOR_TEST,
            )
            .await;
        }
        .boxed()
    })));
    actions.push(Action::SimulateWaitPeriodForOtherProtocol);

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
        None,
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
                compiled_class_hash: compiled_class_hash!(1_u8),
            })),
            Some(StateDiffChunk::DeclaredClass(DeclaredClass {
                class_hash: ClassHash::default(),
                compiled_class_hash: compiled_class_hash!(2_u8),
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
        Action::RunP2pSync,
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
        None,
        actions,
    )
    .await;
}
