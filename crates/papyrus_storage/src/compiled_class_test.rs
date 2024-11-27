use assert_matches::assert_matches;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use pretty_assertions::assert_eq;
use starknet_api::contract_class::SierraVersion;
use starknet_api::core::ClassHash;
use starknet_api::test_utils::read_json_file;

use crate::compiled_class::{CasmStorageReader, CasmStorageWriter};
use crate::db::{DbError, KeyAlreadyExistsError};
use crate::test_utils::get_test_storage;
use crate::StorageError;

#[test]
fn append_versioned_casm() {
    let casm_json = read_json_file("compiled_class.json");
    let expected_casm: CasmContractClass = serde_json::from_value(casm_json).unwrap();
    let ((reader, mut writer), _temp_dir) = get_test_storage();

    writer
        .begin_rw_txn()
        .unwrap()
        .append_versioned_casm(&ClassHash::default(), &(&expected_casm, SierraVersion::default()))
        .unwrap()
        .commit()
        .unwrap();

    let versioned_casm =
        reader.begin_ro_txn().unwrap().get_versioned_casm(&ClassHash::default()).unwrap().unwrap();
    assert_eq!(versioned_casm, (expected_casm, SierraVersion::default()));
}

#[test]
fn casm_rewrite() {
    let ((_, mut writer), _temp_dir) = get_test_storage();

    writer
        .begin_rw_txn()
        .unwrap()
        .append_versioned_casm(
            &ClassHash::default(),
            &(
                &CasmContractClass {
                    prime: Default::default(),
                    compiler_version: Default::default(),
                    bytecode: Default::default(),
                    bytecode_segment_lengths: Default::default(),
                    hints: Default::default(),
                    pythonic_hints: Default::default(),
                    entry_points_by_type: Default::default(),
                },
                SierraVersion::default(),
            ),
        )
        .unwrap()
        .commit()
        .unwrap();

    let Err(err) = writer.begin_rw_txn().unwrap().append_versioned_casm(
        &ClassHash::default(),
        &(
            &CasmContractClass {
                prime: Default::default(),
                compiler_version: Default::default(),
                bytecode: Default::default(),
                bytecode_segment_lengths: Default::default(),
                hints: Default::default(),
                pythonic_hints: Default::default(),
                entry_points_by_type: Default::default(),
            },
            SierraVersion::default(),
        ),
    ) else {
        panic!("Unexpected Ok.");
    };

    assert_matches!(err, StorageError::InnerError(DbError::KeyAlreadyExists(KeyAlreadyExistsError {
        table_name: _,
        key,
        value: _
    })) if key == format!("{:?}", ClassHash::default()));
}
