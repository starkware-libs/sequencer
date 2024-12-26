use std::env;
use std::path::Path;

use assert_matches::assert_matches;
use bytes::Bytes;
use cairo_lang_casm::hints::{Hint, PythonicHint};
use cairo_lang_starknet_classes::abi::Contract;
use cairo_lang_starknet_classes::casm_contract_class::{
    CasmContractClass,
    CasmContractEntryPoints,
};
use cairo_lang_starknet_classes::contract_class::{
    ContractClass,
    ContractClass as CairoLangContractClass,
};
use cairo_lang_starknet_classes::NestedIntList;
use cairo_lang_utils::bigint::BigUintAsHex;
use infra_utils::path::resolve_project_relative_path;
use mempool_test_utils::{FAULTY_ACCOUNT_CLASS_FILE, TEST_FILES_FOLDER};
use num_bigint::BigUint;
use rstest::rstest;
use serde::{Deserialize, Serialize};
use starknet_api::contract_class::{ContractClass, SierraVersion};
use starknet_api::core::CompiledClassHash;
use starknet_api::state::SierraContractClass;

use crate::command_line_compiler::CommandLineCompiler;
use crate::config::{
    SierraCompilationConfig,
    DEFAULT_MAX_CASM_BYTECODE_SIZE,
    DEFAULT_MAX_CPU_TIME,
    DEFAULT_MAX_MEMORY_USAGE,
    DEFAULT_MAX_NATIVE_BYTECODE_SIZE,
};
use crate::errors::CompilationUtilError;
use crate::test_utils::contract_class_from_file;
#[cfg(feature = "cairo_native")]
use crate::SierraToNativeCompiler;
use crate::{SierraCompiler, SierraToCasmCompiler};

const SIERRA_COMPILATION_CONFIG: SierraCompilationConfig = SierraCompilationConfig {
    max_casm_bytecode_size: DEFAULT_MAX_CASM_BYTECODE_SIZE,
    sierra_to_native_compiler_path: None,
    libcairo_native_runtime_path: None,
    max_native_bytecode_size: DEFAULT_MAX_NATIVE_BYTECODE_SIZE,
    max_cpu_time: DEFAULT_MAX_CPU_TIME,
    max_memory_usage: DEFAULT_MAX_MEMORY_USAGE,
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

fn get_faulty_test_contract() -> CairoLangContractClass {
    let mut contract_class = get_test_contract();
    // Truncate the sierra program to trigger an error.
    contract_class.sierra_program = contract_class.sierra_program[..100].to_vec();
    contract_class
}

#[rstest]
#[case::command_line_compiler(command_line_compiler())]
fn test_compile_sierra_to_casm(#[case] compiler: impl SierraToCasmCompiler) {
    let expected_casm_contract_length = 72304;

    let contract_class = get_test_contract();
    let casm_contract = compiler.compile(contract_class).unwrap();
    let serialized_casm = serde_json::to_string_pretty(&casm_contract).unwrap().into_bytes();

    assert_eq!(serialized_casm.len(), expected_casm_contract_length);
}

// TODO(Arni, 1/5/2024): Add a test for panic result test.
#[rstest]
#[case::command_line_compiler(command_line_compiler())]
fn test_negative_flow_compile_sierra_to_casm(#[case] compiler: impl SierraToCasmCompiler) {
    let contract_class = get_faulty_test_contract();

    let result = compiler.compile(contract_class);
    assert_matches!(result, Err(CompilationUtilError::CompilationError(..)));
}

#[cfg(feature = "cairo_native")]
#[test]
fn test_compile_sierra_to_native() {
    let compiler = command_line_compiler();
    let contract_class = get_test_contract();

    // TODO(Avi, 1/1/2025): Check size/memory/time limits.
    let _native_contract_executor = compiler.compile_to_native(contract_class).unwrap();
}

#[cfg(feature = "cairo_native")]
#[test]
fn test_negative_flow_compile_sierra_to_native() {
    let compiler = command_line_compiler();
    let contract_class = get_faulty_test_contract();

    let result = compiler.compile_to_native(contract_class);
    assert_matches!(result, Err(CompilationUtilError::CompilationError(..)));
}

#[rstest]
fn test_max_casm_bytecode_size() {
    let contract_class = get_test_contract();
    let expected_casm_bytecode_length = 1965;

    // Positive flow.
    let compiler = CommandLineCompiler::new(SierraCompilationConfig {
        max_casm_bytecode_size: expected_casm_bytecode_length,
        ..SierraCompilationConfig::default()
    });
    let casm_contract_class = compiler.compile(contract_class.clone()).expect(
        "Failed to compile contract class. Probably an issue with the max_casm_bytecode_size.",
    );
    assert_eq!(casm_contract_class.bytecode.len(), expected_casm_bytecode_length);

    // Negative flow.
    let compiler = CommandLineCompiler::new(SierraCompilationConfig {
        max_casm_bytecode_size: expected_casm_bytecode_length - 1,
        ..SierraCompilationConfig::default()
    });
    let result = compiler.compile(contract_class);
    assert_matches!(result, Err(CompilationUtilError::CompilationError(string))
        if string.contains("Code size limit exceeded.")
    );
}

// TODO: mock compiler.
#[test]
fn test_sierra_compiler() {
    let inner_compiler = command_line_compiler();
    let compiler = SierraCompiler::new(inner_compiler.clone());
    let inner_compiler_compatible_class = get_test_contract();
    let class = SierraContractClass::from(inner_compiler_compatible_class.clone());
    let raw_class = serde_json::to_vec(&class).unwrap().into();

    let (raw_executable_class, executable_class_hash) = compiler.compile(raw_class).unwrap();
    dbg!(raw_executable_class.clone());
    dbg!(raw_executable_class.as_ref());
    // let executable_class: ContractClass =
    //     serde_json::from_slice(raw_executable_class.as_ref()).unwrap();
    let executable_class: CasmContractClass =
        bincode::deserialize(raw_executable_class.as_ref()).unwrap();
    dbg!(executable_class.clone());
    // let expected_executable_class =
    //     inner_compiler.compile(inner_compiler_compatible_class).unwrap();
    // let expected_sierra_version =
    //     SierraVersion::extract_from_program(&class.sierra_program).unwrap();
    // let expected_executable_class =
    //     ContractClass::V1((expected_executable_class, expected_sierra_version));
    // assert_eq!(executable_class, expected_executable_class);
    // assert_eq!(executable_class_hash, expected_executable_class.compiled_class_hash());
}

fn skip_if_none<T>(opt_field: &Option<T>) -> bool {
    opt_field.is_none()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct T {
    #[serde(skip_serializing_if = "skip_if_none")]
    pub x: Option<Vec<usize>>,
}

#[test]
fn test_nested_list() {
    let list = T { x: None };
    let bytes: Bytes = bincode::serialize(&list).unwrap().into();
    dbg!(bytes.clone());
    let list: T = bincode::deserialize(bytes.as_ref()).unwrap();
    dbg!(list);
}
