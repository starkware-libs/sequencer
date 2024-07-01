use std::env;
use std::path::Path;

use assert_matches::assert_matches;
use cairo_lang_starknet_classes::allowed_libfuncs::AllowedLibfuncsError;
use test_utils::{get_absolute_path, FAULTY_ACCOUNT_CLASS_FILE, TEST_FILES_FOLDER};

use crate::compile::{compile_sierra_to_casm, CompilationUtilError};
use crate::test_utils::contract_class_from_file;

#[test]
fn test_compile_sierra_to_casm() {
    env::set_current_dir(get_absolute_path(TEST_FILES_FOLDER)).expect("Failed to set current dir.");
    let sierra_path = Path::new(FAULTY_ACCOUNT_CLASS_FILE);
    let expected_casm_contract_length = 72304;

    let contract_class = contract_class_from_file(sierra_path);
    let casm_contract = compile_sierra_to_casm(contract_class).unwrap();
    let serialized_casm = serde_json::to_string_pretty(&casm_contract).unwrap().into_bytes();

    assert_eq!(serialized_casm.len(), expected_casm_contract_length);
}

// TODO(Arni, 1/5/2024): Add a test for panic result test.
#[test]
fn test_negative_flow_compile_sierra_to_casm() {
    env::set_current_dir(get_absolute_path(TEST_FILES_FOLDER)).expect("Failed to set current dir.");
    let sierra_path = Path::new(FAULTY_ACCOUNT_CLASS_FILE);

    let mut contract_class = contract_class_from_file(sierra_path);
    // Truncate the sierra program to trigger an error.
    contract_class.sierra_program = contract_class.sierra_program[..100].to_vec();

    let result = compile_sierra_to_casm(contract_class);
    assert_matches!(
        result,
        Err(CompilationUtilError::AllowedLibfuncsError(AllowedLibfuncsError::SierraProgramError))
    );
}
