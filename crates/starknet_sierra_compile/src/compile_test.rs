use std::env;
use std::path::Path;

use assert_matches::assert_matches;
use cairo_lang_starknet_classes::contract_class::ContractClass;
use infra_utils::path::get_absolute_path;
use mempool_test_utils::{FAULTY_ACCOUNT_CLASS_FILE, TEST_FILES_FOLDER};
use rstest::rstest;

use crate::cairo_lang_compiler::CairoLangSierraToCasmCompiler;
use crate::command_line_compiler::CommandLineCompiler;
use crate::config::SierraToCasmCompilationConfig;
use crate::errors::CompilationUtilError;
use crate::test_utils::contract_class_from_file;
use crate::SierraToCasmCompiler;
#[cfg(feature = "cairo_native")]
use crate::SierraToNativeCompiler;

const SIERRA_TO_CASM_COMPILATION_CONFIG: SierraToCasmCompilationConfig =
    SierraToCasmCompilationConfig { max_bytecode_size: 81920 };

fn cairo_lang_compiler() -> CairoLangSierraToCasmCompiler {
    CairoLangSierraToCasmCompiler { config: SIERRA_TO_CASM_COMPILATION_CONFIG }
}
fn command_line_compiler() -> CommandLineCompiler {
    CommandLineCompiler::new(SIERRA_TO_CASM_COMPILATION_CONFIG)
}
fn get_test_contract() -> ContractClass {
    env::set_current_dir(get_absolute_path(TEST_FILES_FOLDER)).expect("Failed to set current dir.");
    let sierra_path = Path::new(FAULTY_ACCOUNT_CLASS_FILE);
    contract_class_from_file(sierra_path)
}

fn get_faulty_test_contract() -> ContractClass {
    let mut contract_class = get_test_contract();
    // Truncate the sierra program to trigger an error.
    contract_class.sierra_program = contract_class.sierra_program[..100].to_vec();
    contract_class
}

// TODO: use the other compiler as well.
#[rstest]
#[case::cairo_lang_compiler(cairo_lang_compiler())]
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
#[case::cairo_lang_compiler(cairo_lang_compiler())]
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
