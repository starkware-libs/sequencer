#![cfg(feature = "cairo_native")]
use std::env;
use std::path::Path;

use assert_matches::assert_matches;
use cairo_lang_starknet_classes::contract_class::ContractClass;
use mempool_test_utils::{FAULTY_ACCOUNT_CLASS_FILE, TEST_FILES_FOLDER};
use starknet_compilation_utils::errors::CompilationUtilError;
use starknet_compilation_utils::test_utils::contract_class_from_file;
use starknet_infra_utils::path::resolve_project_relative_path;

use crate::command_line_compiler::CommandLineCompiler;
use crate::config::{
    SierraCompilationConfig,
    DEFAULT_MAX_CPU_TIME,
    DEFAULT_MAX_FILE_SIZE,
    DEFAULT_MAX_MEMORY_USAGE,
    DEFAULT_OPTIMIZATION_LEVEL,
};
use crate::SierraToNativeCompiler;

const SIERRA_COMPILATION_CONFIG: SierraCompilationConfig = SierraCompilationConfig {
    compiler_binary_path: None,
    max_file_size: DEFAULT_MAX_FILE_SIZE,
    max_cpu_time: DEFAULT_MAX_CPU_TIME,
    max_memory_usage: DEFAULT_MAX_MEMORY_USAGE,
    optimization_level: DEFAULT_OPTIMIZATION_LEVEL,
};

fn command_line_compiler() -> CommandLineCompiler {
    CommandLineCompiler::new(SIERRA_COMPILATION_CONFIG)
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
fn test_compile_sierra_to_native() {
    let compiler = command_line_compiler();
    let contract_class = get_test_contract();

    let _native_contract_executor = compiler.compile(contract_class).unwrap();
}

#[test]
fn test_negative_flow_compile_sierra_to_native() {
    let compiler = command_line_compiler();
    let contract_class = get_faulty_test_contract();

    let result = compiler.compile(contract_class);
    assert_matches!(result, Err(CompilationUtilError::CompilationError(..)));
}
