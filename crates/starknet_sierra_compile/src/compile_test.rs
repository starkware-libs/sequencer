use std::env;
use std::path::Path;

use assert_matches::assert_matches;
use cairo_lang_starknet_classes::allowed_libfuncs::AllowedLibfuncsError;
use mempool_test_utils::{get_absolute_path, FAULTY_ACCOUNT_CLASS_FILE, TEST_FILES_FOLDER};
use rstest::{fixture, rstest};

use crate::cairo_lang_compiler::{CairoLangCompiler, CompilationUtilError};
use crate::config::SierraToCasmCompilationConfig;
use crate::test_utils::contract_class_from_file;
use crate::SierraToCasmCompiler;

#[fixture]
fn compiler() -> impl SierraToCasmCompiler {
    CairoLangCompiler { config: SierraToCasmCompilationConfig { max_bytecode_size: 81920 } }
}

#[rstest]
fn test_compile_sierra_to_casm(compiler: impl SierraToCasmCompiler) {
    env::set_current_dir(get_absolute_path(TEST_FILES_FOLDER)).expect("Failed to set current dir.");
    let sierra_path = Path::new(FAULTY_ACCOUNT_CLASS_FILE);
    let expected_casm_contract_length = 72304;

    let contract_class = contract_class_from_file(sierra_path);
    let casm_contract = compiler.compile_sierra_to_casm(contract_class).unwrap();
    let serialized_casm = serde_json::to_string_pretty(&casm_contract).unwrap().into_bytes();

    assert_eq!(serialized_casm.len(), expected_casm_contract_length);
}

// TODO(Arni, 1/5/2024): Add a test for panic result test.
#[rstest]
fn test_negative_flow_compile_sierra_to_casm(compiler: impl SierraToCasmCompiler) {
    env::set_current_dir(get_absolute_path(TEST_FILES_FOLDER)).expect("Failed to set current dir.");
    let sierra_path = Path::new(FAULTY_ACCOUNT_CLASS_FILE);

    let mut contract_class = contract_class_from_file(sierra_path);
    // Truncate the sierra program to trigger an error.
    contract_class.sierra_program = contract_class.sierra_program[..100].to_vec();

    let result = compiler.compile_sierra_to_casm(contract_class);
    assert_matches!(
        result,
        Err(CompilationUtilError::AllowedLibfuncsError(AllowedLibfuncsError::SierraProgramError))
    );
}
