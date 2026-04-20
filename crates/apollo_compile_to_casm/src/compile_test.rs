use std::collections::HashSet;

use apollo_compilation_utils::errors::CompilationUtilError;
use apollo_compilation_utils::test_utils::contract_class_from_file;
use apollo_infra_utils::path::resolve_project_relative_path;
use apollo_sierra_compilation_config::config::{
    SierraCompilationConfig,
    DEFAULT_MAX_BYTECODE_SIZE,
    DEFAULT_MAX_CPU_TIME,
    DEFAULT_MAX_MEMORY_USAGE,
};
use assert_matches::assert_matches;
use cairo_lang_starknet_classes::allowed_libfuncs::{
    lookup_allowed_libfuncs_list,
    AllowedLibfuncs,
    ListSelector,
    BUILTIN_AUDITED_LIBFUNCS_LIST,
};
use cairo_lang_starknet_classes::contract_class::ContractClass as CairoLangContractClass;
use expect_test::expect;
use mempool_test_utils::{FAULTY_ACCOUNT_CLASS_FILE, TEST_FILES_FOLDER};
use pretty_assertions::assert_eq;
use regex::Regex;
use starknet_api::contract_class::ContractClass;
use starknet_api::state::SierraContractClass;

use crate::compiler::SierraToCasmCompiler;
use crate::{RawClass, SierraCompiler};

const SIERRA_COMPILATION_CONFIG: SierraCompilationConfig = SierraCompilationConfig {
    max_bytecode_size: DEFAULT_MAX_BYTECODE_SIZE,
    max_memory_usage: DEFAULT_MAX_MEMORY_USAGE,
    max_cpu_time: DEFAULT_MAX_CPU_TIME,
    audited_libfuncs_only: false,
};

// Libfuncs in allowed_libfuncs.json but not yet in Cairo's audited list.
// Remove entries once they're added to the audited list.
const PENDING_LIBFUNCS: &[&str] = &[];

fn compiler() -> SierraToCasmCompiler {
    SierraToCasmCompiler::new(SIERRA_COMPILATION_CONFIG)
}

fn get_test_contract() -> CairoLangContractClass {
    let sierra_path =
        resolve_project_relative_path(TEST_FILES_FOLDER).unwrap().join(FAULTY_ACCOUNT_CLASS_FILE);
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

#[test]
fn test_max_bytecode_size() {
    let contract_class = get_test_contract();
    let expected_casm_bytecode_length = 1965;

    // Positive flow.
    let compiler = SierraToCasmCompiler::new(SierraCompilationConfig {
        max_bytecode_size: expected_casm_bytecode_length,
        max_memory_usage: DEFAULT_MAX_MEMORY_USAGE,
        max_cpu_time: DEFAULT_MAX_CPU_TIME,
        audited_libfuncs_only: false,
    });
    let casm_contract_class = compiler
        .compile(contract_class.clone())
        .expect("Failed to compile contract class. Probably an issue with the max_bytecode_size.");
    assert_eq!(casm_contract_class.bytecode.len(), expected_casm_bytecode_length);

    // Negative flow.
    let compiler = SierraToCasmCompiler::new(SierraCompilationConfig {
        max_bytecode_size: expected_casm_bytecode_length - 1,
        max_memory_usage: DEFAULT_MAX_MEMORY_USAGE,
        max_cpu_time: DEFAULT_MAX_CPU_TIME,
        audited_libfuncs_only: false,
    });
    let result = compiler.compile(contract_class);
    assert_matches!(result, Err(CompilationUtilError::CompilationError(string))
        if string.contains("Code size limit exceeded.")
    );
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
    let sierra_version = class.get_sierra_version().unwrap();
    let expected_executable_class = ContractClass::V1((expected_executable_class, sierra_version));

    // Test.
    let raw_class = RawClass::try_from(class).unwrap();
    let (raw_executable_class, executable_class_hash) = compiler.compile(raw_class).unwrap();
    let executable_class = ContractClass::try_from(raw_executable_class).unwrap();

    // Assert.
    assert_eq!(executable_class, expected_executable_class);
    assert_eq!(executable_class_hash, expected_executable_class.compiled_class_hash());
}

#[test]
fn allowed_libfuncs_aligned_to_audited() {
    let libfuncs_list_selector = ListSelector::ListName(BUILTIN_AUDITED_LIBFUNCS_LIST.to_string());
    let expected = lookup_allowed_libfuncs_list(libfuncs_list_selector).unwrap().allowed_libfuncs;

    let actual_str = include_str!("allowed_libfuncs.json");
    let actual = serde_json::from_str::<AllowedLibfuncs>(actual_str).unwrap().allowed_libfuncs;

    let pending_set: HashSet<&str> = PENDING_LIBFUNCS.iter().copied().collect();

    let missing: Vec<_> =
        expected.keys().filter(|k| !actual.contains_key(k)).map(ToString::to_string).collect();
    let extra: Vec<_> = actual
        .keys()
        .filter(|k| !expected.contains_key(k))
        .map(ToString::to_string)
        .filter(|k| !pending_set.contains(k.as_str()))
        .collect();
    let mismatched: Vec<_> = expected
        .iter()
        .filter(|(k, v)| actual.get(k).is_some_and(|av| av != *v))
        .map(|(k, _)| k.to_string())
        .collect();

    assert!(
        missing.is_empty() && extra.is_empty() && mismatched.is_empty(),
        "allowed_libfuncs.json is not aligned with the audited list.\n Missing (in audited but \
         not in json): {missing:?}\n Extra (in json but not in audited): {extra:?}\n Value \
         mismatch: {mismatched:?}"
    );
}

#[test]
fn test_max_memory_usage() {
    let contract_class = get_test_contract();

    // Compile the contract class without any memory usage limit to get the expected output.
    let compiler = compiler();
    let expected_executable_class = compiler.compile(contract_class.clone()).unwrap();

    // Positive flow.
    let compiler = SierraToCasmCompiler::new(SierraCompilationConfig {
        max_bytecode_size: DEFAULT_MAX_BYTECODE_SIZE,
        max_memory_usage: DEFAULT_MAX_MEMORY_USAGE,
        max_cpu_time: DEFAULT_MAX_CPU_TIME,
        audited_libfuncs_only: false,
    });
    let executable_class = compiler.compile(contract_class.clone()).unwrap();
    assert_eq!(executable_class, expected_executable_class);

    // Negative flow.
    let compiler = SierraToCasmCompiler::new(SierraCompilationConfig {
        max_bytecode_size: DEFAULT_MAX_BYTECODE_SIZE,
        max_memory_usage: 8 * 1024 * 1024,
        max_cpu_time: DEFAULT_MAX_CPU_TIME,
        audited_libfuncs_only: false,
    });
    let compilation_result = compiler.compile(contract_class);
    let expected_error_pattern = Regex::new(r"memory allocation .*fail").unwrap();
    assert_matches!(compilation_result, Err(CompilationUtilError::CompilationError(string))
        if expected_error_pattern.is_match(&string)
    );
}

// TODO(Noamsp): Add a test to ensure that applying resource limits doesn't corrupt the
// compilation process output.

/// Regression tests: verify that compiler error messages are clean (no resource limit setup noise,
/// no stack backtrace frames). Uses `expect!` snapshots for exact matching.
///
/// If these tests break after a compiler or error-formatting change, run with `UPDATE_EXPECT=1`:
///   UPDATE_EXPECT=1 cargo test -p apollo_compile_to_casm -- faulty_contract_error_message
///   UPDATE_EXPECT=1 cargo test -p apollo_compile_to_casm -- memory_limit_error_message
/// This auto-updates the inline snapshots. Review the diff to confirm the new message is clean.
#[test]
fn faulty_contract_error_message() {
    let compiler = compiler();
    let contract_class = get_faulty_test_contract();

    let error_message = match compiler.compile(contract_class) {
        Err(CompilationUtilError::CompilationError(msg)) => msg,
        other => panic!("Expected CompilationError, got: {other:?}"),
    };

    let expected = expect!["Error: Invalid input for deserialization."];
    expected.assert_eq(&error_message);
}

#[test]
fn memory_limit_error_message() {
    let compiler = SierraToCasmCompiler::new(SierraCompilationConfig {
        max_bytecode_size: DEFAULT_MAX_BYTECODE_SIZE,
        max_memory_usage: 8 * 1024 * 1024,
        max_cpu_time: DEFAULT_MAX_CPU_TIME,
        audited_libfuncs_only: false,
    });
    let contract_class = get_test_contract();

    let error_message = match compiler.compile(contract_class) {
        Err(CompilationUtilError::CompilationError(msg)) => msg,
        other => panic!("Expected CompilationError, got: {other:?}"),
    };

    let expected = expect![[r#"
            memory allocation of 142960 bytes failed
            note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace (process terminated by signal 6)"#]];
    expected.assert_eq(&error_message);
}
