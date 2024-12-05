use std::collections::HashMap;

use futures::FutureExt;
use papyrus_common::pending_classes::ApiContractClass;
use papyrus_protobuf::sync::{
    BlockHashOrNumber,
    DataOrFin,
    DeclaredClass,
    DeprecatedDeclaredClass,
    Direction,
    Query,
    StateDiffChunk,
};
use papyrus_storage::class::ClassStorageReader;
use papyrus_test_utils::{get_rng, GetTestInstance};
use rand::{Rng, RngCore};
use rand_chacha::ChaCha8Rng;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
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

    let mut actions = vec![
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
    for state_diffs_and_classes in &state_diffs_and_classes_of_blocks {
        for (state_diff, _) in state_diffs_and_classes {
            actions.push(Action::SendStateDiff(DataOrFin(Some(state_diff.clone()))));
        }
    }

    let len = state_diffs_and_classes_of_blocks.len();
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
                        reader.begin_ro_txn().unwrap().get_class_marker().unwrap().0
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

                let txn = reader.begin_ro_txn().unwrap();
                for (state_diff, expected_class) in state_diffs_and_classes {
                    let class_hash = state_diff.get_class_hash();
                    match expected_class {
                        ApiContractClass::ContractClass(expected_class) => {
                            let actual_class = txn.get_class(&class_hash).unwrap().unwrap();
                            assert_eq!(actual_class, expected_class.clone());
                        }
                        ApiContractClass::DeprecatedContractClass(expected_class) => {
                            let actual_class =
                                txn.get_deprecated_class(&class_hash).unwrap().unwrap();
                            assert_eq!(actual_class, expected_class.clone());
                        }
                    }
                }
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
        actions,
    )
    .await;
}

// We define this new trait here so we can use the get_class_hash function in the test.
// we need to define this trait because StateDiffChunk is defined in an other crate.
trait GetClassHash {
    fn get_class_hash(&self) -> ClassHash;
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
        (
            StateDiffChunk::DeclaredClass(declared_class),
            // TODO(noamsp): get_test_instance on these types returns the same value, making this
            // test redundant. Fix this.
            ApiContractClass::ContractClass(SierraContractClass::get_test_instance(rng)),
        )
    } else {
        let deprecated_declared_class = DeprecatedDeclaredClass { class_hash };
        (
            StateDiffChunk::DeprecatedDeclaredClass(deprecated_declared_class),
            ApiContractClass::DeprecatedContractClass(DeprecatedContractClass::get_test_instance(
                rng,
            )),
        )
    }
}
