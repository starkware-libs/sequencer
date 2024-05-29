use std::path::Path;

use assert_matches::assert_matches;
use cairo_lang_starknet_classes::allowed_libfuncs::AllowedLibfuncsError;

use crate::compile::{compile_sierra_to_casm, CompilationUtilError};
use crate::test_utils::contract_class_from_file;

const FAULTY_ACCOUNT_SIERRA_FILE: &str = "account_faulty.sierra.json";
const TEST_FILES_FOLDER: &str = "./tests/fixtures";

#[test]
fn test_compile_sierra_to_casm() {
    let sierra_path = &Path::new(TEST_FILES_FOLDER).join(FAULTY_ACCOUNT_SIERRA_FILE);
    let expected_casm_contract_length = 72304;

    let contract_class = contract_class_from_file(sierra_path);
    let casm_contract = compile_sierra_to_casm(contract_class).unwrap();
    let serialized_casm = serde_json::to_string_pretty(&casm_contract).unwrap().into_bytes();

    assert_eq!(serialized_casm.len(), expected_casm_contract_length);
}

// TODO(Arni, 1/5/2024): Add a test for panic result test.
#[test]
fn test_negative_flow_compile_sierra_to_casm() {
    let sierra_path = &Path::new(TEST_FILES_FOLDER).join(FAULTY_ACCOUNT_SIERRA_FILE);

    let mut contract_class = contract_class_from_file(sierra_path);
    // Truncate the sierra program to trigger an error.
    contract_class.sierra_program = contract_class.sierra_program[..100].to_vec();

    let result = compile_sierra_to_casm(contract_class);
    assert_matches!(
        result,
        Err(CompilationUtilError::AllowedLibfuncsError(AllowedLibfuncsError::SierraProgramError))
    );
}
