use apollo_compilation_utils::errors::CompilationUtilError;
use apollo_compilation_utils::test_utils::contract_class_from_file;
use apollo_infra_utils::path::resolve_project_relative_path;
use assert_matches::assert_matches;
use cairo_lang_starknet_classes::contract_class::ContractClass;
use mempool_test_utils::{FAULTY_ACCOUNT_CLASS_FILE, TEST_FILES_FOLDER};

use crate::compiler::SierraToNativeCompiler;
use crate::config::SierraCompilationConfig;

fn compiler() -> SierraToNativeCompiler {
    SierraToNativeCompiler::new(SierraCompilationConfig::create_for_testing())
}

fn get_test_contract() -> ContractClass {
    let sierra_path =
        resolve_project_relative_path(TEST_FILES_FOLDER).unwrap().join(FAULTY_ACCOUNT_CLASS_FILE);
    contract_class_from_file(sierra_path)
}

fn get_faulty_test_contract() -> ContractClass {
    let mut contract_class = get_test_contract();
    // Truncate the sierra program to trigger an error.
    contract_class.sierra_program = contract_class.sierra_program[..100].to_vec();
    contract_class
}

#[test]
fn test_compile_sierra_to_native() {
    let compiler = compiler();
    let contract_class = get_test_contract();

    let _native_contract_executor = compiler.compile(contract_class).unwrap();
}

#[test]
fn test_negative_flow_compile_sierra_to_native() {
    let compiler = compiler();
    let contract_class = get_faulty_test_contract();

    let result = compiler.compile(contract_class);
    assert_matches!(result, Err(CompilationUtilError::CompilationError(..)));
}
