use apollo_test_utils::{get_rng, GetTestInstance};
use assert_matches::assert_matches;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use pretty_assertions::assert_eq;
use rstest::rstest;
use starknet_api::block::BlockNumber;
use starknet_api::core::ClassHash;
use starknet_api::state::SierraContractClass;
use starknet_api::test_utils::read_json_file;

use crate::class::ClassStorageWriter;
use crate::compiled_class::{CasmStorageReader, CasmStorageWriter};
use crate::db::{DbError, KeyAlreadyExistsError};
use crate::test_utils::get_test_storage;
use crate::StorageError;

#[test]
fn append_casm() {
    let expected_casm: CasmContractClass = read_json_file("compiled_class.json");
    let ((reader, mut writer), _temp_dir) = get_test_storage();

    writer
        .begin_rw_txn()
        .unwrap()
        .append_casm(&ClassHash::default(), &expected_casm)
        .unwrap()
        .commit()
        .unwrap();

    let casm = reader.begin_ro_txn().unwrap().get_casm(&ClassHash::default()).unwrap().unwrap();
    assert_eq!(casm, expected_casm);
}

#[rstest]
fn test_casm_and_sierra(
    #[values(true, false)] has_casm: bool,
    #[values(true, false)] has_sierra: bool,
) {
    let test_class_hash = ClassHash::default();
    let mut rng = get_rng();

    // Setup storage.
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    let expected_casm = CasmContractClass::get_test_instance(&mut rng);
    let expected_sierra = <SierraContractClass as GetTestInstance>::get_test_instance(&mut rng);

    if has_casm {
        writer
            .begin_rw_txn()
            .unwrap()
            .append_casm(&test_class_hash, &expected_casm)
            .unwrap()
            .commit()
            .unwrap();
    }
    if has_sierra {
        writer
            .begin_rw_txn()
            .unwrap()
            .append_classes(BlockNumber::default(), &[(test_class_hash, &expected_sierra)], &[])
            .unwrap()
            .commit()
            .unwrap();
    }

    let result = reader.begin_ro_txn().unwrap().get_casm_and_sierra(&test_class_hash);

    assert_eq!(
        result.unwrap(),
        (has_casm.then_some(expected_casm), has_sierra.then_some(expected_sierra))
    );
}

#[test]
fn casm_rewrite() {
    let ((_, mut writer), _temp_dir) = get_test_storage();

    writer
        .begin_rw_txn()
        .unwrap()
        .append_casm(
            &ClassHash::default(),
            &CasmContractClass {
                prime: Default::default(),
                compiler_version: Default::default(),
                bytecode: Default::default(),
                bytecode_segment_lengths: Default::default(),
                hints: Default::default(),
                pythonic_hints: Default::default(),
                entry_points_by_type: Default::default(),
            },
        )
        .unwrap()
        .commit()
        .unwrap();

    let Err(err) = writer.begin_rw_txn().unwrap().append_casm(
        &ClassHash::default(),
        &CasmContractClass {
            prime: Default::default(),
            compiler_version: Default::default(),
            bytecode: Default::default(),
            bytecode_segment_lengths: Default::default(),
            hints: Default::default(),
            pythonic_hints: Default::default(),
            entry_points_by_type: Default::default(),
        },
    ) else {
        panic!("Unexpected Ok.");
    };

    assert_matches!(err, StorageError::InnerError(DbError::KeyAlreadyExists(KeyAlreadyExistsError {
        table_name: _,
        key,
        value: _
    })) if key == format!("{:?}", ClassHash::default()));
}
