use std::collections::HashMap;

use assert_matches::assert_matches;
use indexmap::indexmap;
use pretty_assertions::assert_eq;
use starknet_api::block::BlockNumber;
use starknet_api::compiled_class_hash;
use starknet_api::core::ClassHash;
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::hash::StarkHash;
use starknet_api::state::{SierraContractClass, StateNumber, ThinStateDiff};
use starknet_api::test_utils::read_json_file;

use super::{get_declared_class_hash_to_component_hashes, ClassStorageReader, ClassStorageWriter};
use crate::state::{StateStorageReader, StateStorageWriter};
use crate::test_utils::get_test_storage;
use crate::{MarkerKind, StorageError};

#[test]
fn append_classes_writes_correct_data() {
    let expected_class: SierraContractClass = read_json_file("class.json");
    let expected_deprecated_class: DeprecatedContractClass =
        read_json_file("deprecated_class.json");
    let class_hash = ClassHash::default();
    let deprecated_class_hash = ClassHash(StarkHash::ONE);

    let ((reader, mut writer), _temp_dir) = get_test_storage();

    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(
            BlockNumber(0),
            ThinStateDiff {
                class_hash_to_compiled_class_hash: indexmap! { class_hash => compiled_class_hash!(1_u8) },
                deprecated_declared_classes: vec![deprecated_class_hash],
                ..Default::default()
            },
        )
        .unwrap()
        .append_classes(
            BlockNumber(0),
            &[(class_hash, &expected_class)],
            &[(deprecated_class_hash, &expected_deprecated_class)],
        )
        .unwrap()
        .commit()
        .unwrap();

    let class = reader.begin_ro_txn().unwrap().get_class(&ClassHash::default()).unwrap().unwrap();
    assert_eq!(class, expected_class);

    let deprecated_class = reader
        .begin_ro_txn()
        .unwrap()
        .get_deprecated_class(&deprecated_class_hash)
        .unwrap()
        .unwrap();
    assert_eq!(deprecated_class, expected_deprecated_class);
}

#[test]
fn append_classes_marker_mismatch() {
    let ((_reader, mut writer), _temp_dir) = get_test_storage();

    let Err(err) = writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), ThinStateDiff::default())
        .unwrap()
        .append_classes(BlockNumber(1), &Vec::new(), &Vec::new())
    else {
        panic!("Unexpected Ok.");
    };

    assert_matches!(
        err,
        StorageError::MarkerMismatch { marker_kind: MarkerKind::Class, expected, found } if expected.0 == 0 && found.0 == 1
    );
}

#[test]
fn append_deprecated_class_not_in_state_diff() {
    let expected_deprecated_class: DeprecatedContractClass =
        read_json_file("deprecated_class.json");
    let deprecated_class_hash = ClassHash::default();

    let ((reader, mut writer), _temp_dir) = get_test_storage();

    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(BlockNumber(0), ThinStateDiff::default())
        .unwrap()
        .append_classes(BlockNumber(0), &[], &[])
        .unwrap()
        .append_state_diff(BlockNumber(1), ThinStateDiff::default())
        .unwrap()
        .append_classes(BlockNumber(1), &[], &[(deprecated_class_hash, &expected_deprecated_class)])
        .unwrap()
        .commit()
        .unwrap();

    let txn = reader.begin_ro_txn().unwrap();
    let statetxn = txn.get_state_reader().unwrap();

    let state0 = StateNumber::right_after_block(BlockNumber(0)).unwrap();
    assert!(
        statetxn
            .get_deprecated_class_definition_at(state0, &deprecated_class_hash)
            .unwrap()
            .is_none()
    );

    let state1 = StateNumber::right_after_block(BlockNumber(1)).unwrap();
    assert_eq!(
        statetxn
            .get_deprecated_class_definition_at(state1, &deprecated_class_hash)
            .unwrap()
            .unwrap(),
        expected_deprecated_class
    );
}

/// Verifies that `get_declared_class_hash_to_component_hashes` returns the component hashes of
/// classes freshly declared in a block.
#[test]
fn test_declared_class_hash_to_component_hashes() {
    let class_a = SierraContractClass::default();
    let class_b: SierraContractClass = read_json_file("class.json");
    let class_hash_a = ClassHash::default();
    let class_hash_b = ClassHash(StarkHash::ONE);

    let ((reader, mut writer), _temp_dir) = get_test_storage();

    writer
        .begin_rw_txn()
        .unwrap()
        // Block 0: class A is freshly declared.
        .append_state_diff(
            BlockNumber(0),
            ThinStateDiff {
                class_hash_to_compiled_class_hash: indexmap! {
                    class_hash_a => compiled_class_hash!(1_u8),
                },
                ..Default::default()
            },
        )
        .unwrap()
        .append_classes(BlockNumber(0), &[(class_hash_a, &class_a)], &[])
        .unwrap()
        // Block 1: class B is freshly declared, while class A's compiled class hash is migrated.
        .append_state_diff(
            BlockNumber(1),
            ThinStateDiff {
                class_hash_to_compiled_class_hash: indexmap! {
                    class_hash_a => compiled_class_hash!(2_u8),
                    class_hash_b => compiled_class_hash!(3_u8),
                },
                ..Default::default()
            },
        )
        .unwrap()
        .append_classes(BlockNumber(1), &[(class_hash_b, &class_b)], &[])
        .unwrap()
        .commit()
        .unwrap();

    let txn = reader.begin_ro_txn().unwrap();

    // Block 0: only the freshly declared class A.
    assert_eq!(
        get_declared_class_hash_to_component_hashes(&txn, BlockNumber(0)).unwrap(),
        HashMap::from([(class_hash_a, class_a.get_component_hashes())]),
    );

    // Block 1: only the freshly declared class B; the migrated class A is excluded.
    assert_eq!(
        get_declared_class_hash_to_component_hashes(&txn, BlockNumber(1)).unwrap(),
        HashMap::from([(class_hash_b, class_b.get_component_hashes())]),
    );
}
