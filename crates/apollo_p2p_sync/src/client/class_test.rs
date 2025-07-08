use std::collections::HashMap;

use apollo_class_manager_types::{ClassHashes, MockClassManagerClient};
use apollo_protobuf::sync::{
    BlockHashOrNumber,
    DataOrFin,
    DeclaredClass,
    DeprecatedDeclaredClass,
    Direction,
    Query,
    StateDiffChunk,
};
use apollo_storage::class_manager::ClassManagerStorageReader;
use apollo_test_utils::{get_rng, GetTestInstance};
use futures::FutureExt;
use mockall::predicate::eq;
use papyrus_common::pending_classes::ApiContractClass;
use rand::{Rng, RngCore};
use rand_chacha::ChaCha8Rng;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash, EntryPointSelector};
use starknet_api::deprecated_contract_class::{
    ContractClass as DeprecatedContractClass,
    EntryPointOffset,
    EntryPointV0,
};
use starknet_api::state::SierraContractClass;

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
async fn class_basic_flow() {
    let mut rng = get_rng();

    let mut class_manager_client = MockClassManagerClient::new();

    let state_diffs_and_classes_of_blocks = [
        vec![
            create_random_state_diff_chunk_with_class(&mut rng),
            create_random_state_diff_chunk_with_class(&mut rng),
        ],
        vec![
            create_random_state_diff_chunk_with_class(&mut rng),
            create_random_state_diff_chunk_with_class(&mut rng),
            create_random_state_diff_chunk_with_class(&mut rng),
        ],
    ];

    // Fill class manager client with expectations.
    for state_diffs_and_classes in &state_diffs_and_classes_of_blocks {
        for (state_diff, class) in state_diffs_and_classes {
            let class_hash = state_diff.get_class_hash();
            match class {
                ApiContractClass::ContractClass(class) => {
                    let executable_class_hash_v2 = state_diff.get_compiled_class_hash();
                    class_manager_client
                        .expect_add_class()
                        .times(1)
                        .with(eq(class.clone()))
                        .return_once(move |_| {
                            Ok(ClassHashes { class_hash, executable_class_hash_v2 })
                        });
                }
                ApiContractClass::DeprecatedContractClass(class) => {
                    class_manager_client
                        .expect_add_deprecated_class()
                        .times(1)
                        .with(eq(class_hash), eq(class.clone()))
                        .return_once(|_, _| Ok(()));
                }
            }
        }
    }

    let mut actions = vec![
        Action::RunP2pSync,
        // We already validate the header query content in other tests.
        Action::ReceiveQuery(Box::new(|_query| ()), DataType::Header),
    ];

    // Send headers with corresponding state diff length.
    for (i, state_diffs_and_classes) in state_diffs_and_classes_of_blocks.iter().enumerate() {
        actions.push(Action::SendHeader(DataOrFin(Some(random_header(
            &mut rng,
            BlockNumber(i.try_into().unwrap()),
            Some(state_diffs_and_classes.len()),
            None,
        )))));
    }
    actions.push(Action::SendHeader(DataOrFin(None)));

    // Send state diffs.
    actions.push(
        // We already validate the state diff query content in other tests.
        Action::ReceiveQuery(Box::new(|_query| ()), DataType::StateDiff),
    );

    // Sleep so class sync will reach the sleep waiting for state diff protocol to receive new data.
    actions.push(Action::SleepToLetSyncAdvance);
    for state_diffs_and_classes in &state_diffs_and_classes_of_blocks {
        for (state_diff, _) in state_diffs_and_classes {
            actions.push(Action::SendStateDiff(DataOrFin(Some(state_diff.clone()))));
        }
    }

    let len = state_diffs_and_classes_of_blocks.len();
    // Wait for state diff sync to finish before continuing class sync.
    actions.push(Action::CheckStorage(Box::new(move |reader| {
        async move {
            let block_number = BlockNumber(len.try_into().unwrap());
            wait_for_marker(
                DataType::StateDiff,
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
        DataType::Class,
    ));
    for (i, state_diffs_and_classes) in state_diffs_and_classes_of_blocks.into_iter().enumerate() {
        for (state_diff, class) in &state_diffs_and_classes {
            let class_hash = state_diff.get_class_hash();

            // Check that before the last class was sent, the classes aren't written.
            actions.push(Action::CheckStorage(Box::new(move |reader| {
                async move {
                    assert_eq!(
                        u64::try_from(i).unwrap(),
                        reader.begin_ro_txn().unwrap().get_class_manager_block_marker().unwrap().0
                    );
                }
                .boxed()
            })));
            actions.push(Action::SendClass(DataOrFin(Some((class.clone(), class_hash)))));
        }
        // Check that a block's classes are written before the entire query finished.
        actions.push(Action::CheckStorage(Box::new(move |reader| {
            async move {
                let block_number = BlockNumber(i.try_into().unwrap());
                wait_for_marker(
                    DataType::Class,
                    &reader,
                    block_number.unchecked_next(),
                    SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
                    TIMEOUT_FOR_TEST,
                )
                .await;
            }
            .boxed()
        })));
    }

    run_test(
        HashMap::from([
            (DataType::Header, len.try_into().unwrap()),
            (DataType::StateDiff, len.try_into().unwrap()),
            (DataType::Class, len.try_into().unwrap()),
        ]),
        Some(class_manager_client),
        actions,
    )
    .await;
}

// We define this new trait here so we can use the get_class_hash function in the test.
// we need to define this trait because StateDiffChunk is defined in an other crate.
trait GetClassHash {
    fn get_class_hash(&self) -> ClassHash;
    fn get_compiled_class_hash(&self) -> CompiledClassHash;
}

impl GetClassHash for StateDiffChunk {
    fn get_class_hash(&self) -> ClassHash {
        match self {
            StateDiffChunk::DeclaredClass(declared_class) => declared_class.class_hash,
            StateDiffChunk::DeprecatedDeclaredClass(deprecated_declared_class) => {
                deprecated_declared_class.class_hash
            }
            _ => unreachable!(),
        }
    }

    fn get_compiled_class_hash(&self) -> CompiledClassHash {
        match self {
            StateDiffChunk::DeclaredClass(declared_class) => declared_class.compiled_class_hash,
            _ => unreachable!(),
        }
    }
}

fn create_random_state_diff_chunk_with_class(
    rng: &mut ChaCha8Rng,
) -> (StateDiffChunk, ApiContractClass) {
    let class_hash = ClassHash(rng.next_u64().into());
    if rng.gen_bool(0.5) {
        let declared_class = DeclaredClass {
            class_hash,
            compiled_class_hash: CompiledClassHash(rng.next_u64().into()),
        };

        // SierraContractClass::get_test_instance(rng) currently returns the same value every time,
        // so we change the program to be random.
        let mut sierra_contract_class = SierraContractClass::get_test_instance(rng);

        sierra_contract_class.sierra_program = vec![rng.next_u64().into()];
        (
            StateDiffChunk::DeclaredClass(declared_class),
            ApiContractClass::ContractClass(sierra_contract_class),
        )
    } else {
        let deprecated_declared_class = DeprecatedDeclaredClass { class_hash };

        // DeprecatedContractClass::get_test_instance(rng) currently returns the same value every
        // time, so we change the entry points to be random.
        let mut deprecated_contract_class = DeprecatedContractClass::get_test_instance(rng);
        deprecated_contract_class.entry_points_by_type.insert(
            Default::default(),
            vec![EntryPointV0 {
                selector: EntryPointSelector::default(),
                offset: EntryPointOffset(rng.next_u64().try_into().unwrap()),
            }],
        );

        (
            StateDiffChunk::DeprecatedDeclaredClass(deprecated_declared_class),
            ApiContractClass::DeprecatedContractClass(deprecated_contract_class),
        )
    }
}

// TODO(noamsp): Consider verifying that ParseDataError::BadPeerError(NotEnoughClasses)
// was returned from parse_data_for_block. We currently dont have a way to check this.
#[tokio::test]
async fn not_enough_classes() {
    let mut rng = get_rng();
    let (state_diff_chunk, class) = create_random_state_diff_chunk_with_class(&mut rng);

    validate_class_sync_fails(
        vec![2],
        vec![
            Some(state_diff_chunk.clone()),
            Some(create_random_state_diff_chunk_with_class(&mut rng).0),
        ],
        vec![Some((class, state_diff_chunk.get_class_hash())), None],
    )
    .await;
}

// TODO(noamsp): Consider verifying that ParseDataError::BadPeerError(ClassNotInStateDiff)
// was returned from parse_data_for_block. We currently dont have a way to check this.
#[tokio::test]
async fn class_not_in_state_diff() {
    let mut rng = get_rng();
    let (state_diff_chunk, class) = create_random_state_diff_chunk_with_class(&mut rng);

    validate_class_sync_fails(
        vec![1],
        vec![Some(create_random_state_diff_chunk_with_class(&mut rng).0)],
        vec![Some((class, state_diff_chunk.get_class_hash()))],
    )
    .await;
}

// TODO(noamsp): Consider verifying that ParseDataError::BadPeerError(DuplicateClass)
// was returned from parse_data_for_block. We currently dont have a way to check this.
#[tokio::test]
async fn duplicate_classes() {
    let mut rng = get_rng();
    let (state_diff_chunk, class) = create_random_state_diff_chunk_with_class(&mut rng);

    // We provide a state diff with 3 classes to verify that we return the error once we encounter
    // duplicate classes and not wait for the whole state diff classes to be sent.
    validate_class_sync_fails(
        vec![3],
        vec![
            Some(state_diff_chunk.clone()),
            Some(create_random_state_diff_chunk_with_class(&mut rng).0),
            Some(create_random_state_diff_chunk_with_class(&mut rng).0),
        ],
        vec![
            Some((class.clone(), state_diff_chunk.get_class_hash())),
            Some((class, state_diff_chunk.get_class_hash())),
        ],
    )
    .await;
}

async fn validate_class_sync_fails(
    header_state_diff_lengths: Vec<usize>,
    state_diff_chunks: Vec<Option<StateDiffChunk>>,
    classes: Vec<Option<(ApiContractClass, ClassHash)>>,
) {
    let mut rng = get_rng();

    // TODO(noamsp): remove code duplication with state diff test.
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

    actions.push(Action::SendStateDiff(DataOrFin(None)));

    actions.push(
        // We already validate the class query content in other tests.
        Action::ReceiveQuery(Box::new(|_query| ()), DataType::Class),
    );

    // Send classes.
    for class in classes {
        actions.push(Action::SendClass(DataOrFin(class)));
    }

    // We validate the report is sent before we send fin.
    actions.push(Action::ValidateReportSent(DataType::Class));

    run_test(
        HashMap::from([
            (DataType::Header, header_state_diff_lengths.len().try_into().unwrap()),
            (DataType::StateDiff, header_state_diff_lengths.len().try_into().unwrap()),
            (DataType::Class, header_state_diff_lengths.len().try_into().unwrap()),
        ]),
        None,
        actions,
    )
    .await;
}
