use std::env;
use std::path::Path;

use apollo_infra_utils::path::resolve_project_relative_path;
use assert_matches::assert_matches;
use cairo_lang_starknet_classes::contract_class::ContractClass as CairoLangContractClass;
use mempool_test_utils::{FAULTY_ACCOUNT_CLASS_FILE, TEST_FILES_FOLDER};
use starknet_api::contract_class::{ContractClass, SierraVersion};
use starknet_api::state::SierraContractClass;
use apollo_compilation_utils::errors::CompilationUtilError;
use apollo_compilation_utils::test_utils::contract_class_from_file;

use crate::compiler::SierraToCasmCompiler;
use crate::config::{SierraCompilationConfig, DEFAULT_MAX_BYTECODE_SIZE};
use crate::{RawClass, SierraCompiler};

const SIERRA_COMPILATION_CONFIG: SierraCompilationConfig =
    SierraCompilationConfig { max_bytecode_size: DEFAULT_MAX_BYTECODE_SIZE };

fn compiler() -> SierraToCasmCompiler {
    SierraToCasmCompiler::new(SIERRA_COMPILATION_CONFIG)
}

fn get_test_contract() -> CairoLangContractClass {
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

#[test]
fn test_compile_sierra_to_casm() {
    let compiler = compiler();
    let expected_casm_contract_length = 72305;

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

// TODO(Elin): mock compiler.
#[test]
fn test_sierra_compiler() {
    // Setup.
    let compiler = compiler();
    let class = get_test_contract();
    let expected_executable_class = compiler.compile(class.clone()).unwrap();

    let compiler = SierraCompiler::new(compiler);
    let class = SierraContractClass::from(class);
    let sierra_version = SierraVersion::extract_from_program(&class.sierra_program).unwrap();
    let expected_executable_class = ContractClass::V1((expected_executable_class, sierra_version));

    // Test.
    let raw_class = RawClass::try_from(class).unwrap();
    let (raw_executable_class, executable_class_hash) = compiler.compile(raw_class).unwrap();
    let executable_class = ContractClass::try_from(raw_executable_class).unwrap();

    // Assert.
    assert_eq!(executable_class, expected_executable_class);
    assert_eq!(executable_class_hash, expected_executable_class.compiled_class_hash());
}
