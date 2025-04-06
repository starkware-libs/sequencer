use std::env;
use std::path::Path;

use assert_matches::assert_matches;
use cairo_lang_starknet_classes::contract_class::ContractClass;
use mempool_test_utils::{FAULTY_ACCOUNT_CLASS_FILE, TEST_FILES_FOLDER};
use starknet_compilation_utils::errors::CompilationUtilError;
use starknet_compilation_utils::resource_limits::ResourceLimits;
use starknet_compilation_utils::test_utils::contract_class_from_file;
use starknet_infra_utils::path::resolve_project_relative_path;

use crate::compiler::SierraToCasmCompiler;
use crate::config::{SierraCompilationConfig, DEFAULT_MAX_BYTECODE_SIZE};

const SIERRA_COMPILATION_CONFIG: SierraCompilationConfig =
    SierraCompilationConfig { max_bytecode_size: DEFAULT_MAX_BYTECODE_SIZE };

fn compiler() -> SierraToCasmCompiler {
    SierraToCasmCompiler::new(SIERRA_COMPILATION_CONFIG)
}

fn get_test_contract() -> ContractClass {
    env::set_current_dir(resolve_project_relative_path(TEST_FILES_FOLDER).unwrap())
        .expect("Failed to set current dir.");
    let sierra_path = Path::new(FAULTY_ACCOUNT_CLASS_FILE);
    contract_class_from_file(sierra_path)
}

fn get_faulty_test_contract() -> ContractClass {
    let mut contract_class = get_test_contract();
    // Truncate the sierra program to trigger an error.
    contract_class.sierra_program = contract_class.sierra_program[..100].to_vec();
    contract_class
}

#[test]
fn test_compile_sierra_to_casm() {
    let compiler = compiler();
    let expected_casm_contract_length = 72304;

    let contract_class = get_test_contract();
    let casm_contract = compiler.compile(contract_class).unwrap();
    let serialized_casm = serde_json::to_string_pretty(&casm_contract).unwrap().into_bytes();

    assert_eq!(serialized_casm.len(), expected_casm_contract_length);
}

// TODO(Arni, 1/5/2024): Add a test for panic result test.
#[test]
fn test_negative_flow_compile_sierra_to_casm() {
    let compiler = compiler();
    let contract_class = get_faulty_test_contract();

    let result = compiler.compile(contract_class);
    assert_matches!(result, Err(CompilationUtilError::CompilationError(..)));
}

#[rstest]
#[case::sufficient_resources(None, true)]
#[case::low_memory(Some(1000000), false)]
// CONSIDER(AvivG): Add cases for different resource limits.
fn test_memory_resource_limit(#[case] memory_limit: Option<u64>, #[case] should_succeed: bool) {
    use starknet_compilation_utils::compiler_utils::compile_with_args;

    let contract_class = get_test_contract();
    // TO Fix(AvivG): How to get the compiler binary path?
    // let compiler_binary_path = command_line_compiler().path_to_binary;
    let compiler_binary_path = None;
    let additional_args = vec![];
    let resource_limits = ResourceLimits::new(None, None, memory_limit);

    let result =
        compile_with_args(compiler_binary_path, contract_class, &additional_args, resource_limits);

    if should_succeed {
        assert!(result.is_ok());
    } else {
        assert!(result.is_err());
    }
}
