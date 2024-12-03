use assert_matches::assert_matches;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use papyrus_test_utils::GetTestInstance;
use pretty_assertions::assert_eq;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
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
    let casm_json = read_json_file("compiled_class.json");
    let expected_casm: CasmContractClass = serde_json::from_value(casm_json).unwrap();
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
#[case("both_exist", true, true, None)]
#[case("only_casm_exists", true, false, Some("CASM but not in Sierra"))]
#[case("only_sierra_exists", false, true, Some("Sierra but not in CASM"))]
#[case("neither_exists", false, false, None)]
fn test_casm_and_sierra(
    #[case] test_name: &str,
    #[case] has_casm: bool,
    #[case] has_sierra: bool,
    #[case] expected_error: Option<&str>,
) {
    // Initialize the RNG with a seed
    let mut rng = ChaCha8Rng::seed_from_u64(0);

    let test_class_hash = ClassHash::default();

    // Setup storage
    let ((reader, mut writer), _temp_dir) = get_test_storage();

    //
    let expected_casm = CasmContractClass::get_test_instance(&mut rng);
    let sierra_json = read_json_file("class.json");
    let expected_sierra: SierraContractClass = serde_json::from_value(sierra_json).unwrap();

    // Add CASM if required
    if has_casm {
        writer
            .begin_rw_txn()
            .unwrap()
            .append_casm(&test_class_hash, &expected_casm)
            .unwrap()
            .commit()
            .unwrap();
    }

    // Add Sierra if required
    if has_sierra {
        writer
            .begin_rw_txn()
            .unwrap()
            .append_classes(BlockNumber::default(), &[(test_class_hash, &expected_sierra)], &[])
            .unwrap()
            .commit()
            .unwrap();
    }

    // Call the function being tested
    let result = reader.begin_ro_txn().unwrap().get_casm_and_sierra(&test_class_hash);

    // Handle assertions
    match expected_error {
        Some(expected_message) => {
            dbg!(&result);
            // If Sierra XOR CASM exists
            assert!(matches!(
                result,
                Err(StorageError::DBInconsistency { msg }) if msg.contains(expected_message)
            ));
        }
        None if has_casm && has_sierra => {
            // If both CASM and Sierra exist
            let (casm, sierra) = result.unwrap().unwrap();
            assert_eq!(casm, expected_casm);
            let sierra_json = read_json_file("class.json");
            let expected_sierra: SierraContractClass = serde_json::from_value(sierra_json).unwrap();
            assert_eq!(sierra, expected_sierra);
        }
        None => {
            // If neither CASM nor Sierra exists
            assert!(result.unwrap().is_none());
        }
    }
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
