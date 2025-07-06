use std::collections::HashMap;
use std::env::current_dir;
use std::fs::File;
use std::io::Read;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use starknet_types_core::felt::Felt;

use crate::test_utils::cairo_runner::{
    run_cairo_0_entry_point,
    Cairo0EntryPointRunnerResult,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
    ValueArg,
};

/// Creates a CompiledClass structure from the test contract JSON data using
/// cairo_lang_starknet_classes
fn create_compiled_class_from_test_contract() -> Vec<EndpointArg> {
    // Read the test contract JSON file
    let contract_path = current_dir().unwrap()
        .parent().unwrap() // Go up to 'crates' directory
        .parent().unwrap() // Go up to workspace root
        .join("crates/blockifier_test_utils/resources/feature_contracts/cairo1/compiled/test_contract.casm.json");

    let mut file = File::open(&contract_path)
        .unwrap_or_else(|_| panic!("Unable to open file {contract_path:?}"));
    let mut data = String::new();
    file.read_to_string(&mut data)
        .unwrap_or_else(|_| panic!("Unable to read file {contract_path:?}"));

    // Parse the JSON into a CasmContractClass using cairo_lang_starknet_classes
    let casm_contract_class: CasmContractClass = serde_json::from_str(&data)
        .expect("Failed to parse test contract JSON into CasmContractClass");

    // Create the CompiledClass structure as expected by the OS
    let mut compiled_class_args = Vec::new();

    // compiled_class_version: felt (should be 'COMPILED_CLASS_V1')
    compiled_class_args.push(EndpointArg::from(Felt::from_bytes_be_slice(b"COMPILED_CLASS_V1")));

    // External functions
    let external_functions: Vec<EndpointArg> = casm_contract_class
        .entry_points_by_type
        .external
        .iter()
        .map(|ep| {
            let mut entry_point_args = Vec::new();

            // selector
            entry_point_args.push(EndpointArg::from(Felt::from(&ep.selector)));

            // offset
            entry_point_args.push(EndpointArg::from(ep.offset as u128));

            // n_builtins
            entry_point_args.push(EndpointArg::from(ep.builtins.len() as u128));

            // builtin_list
            let builtins: Vec<Felt> = ep
                .builtins
                .iter()
                .map(|builtin| Felt::from_bytes_be_slice(builtin.as_bytes()))
                .collect();
            entry_point_args.push(EndpointArg::Pointer(PointerArg::Array(builtins)));

            EndpointArg::Pointer(PointerArg::Composed(entry_point_args))
        })
        .collect();

    compiled_class_args.push(EndpointArg::from(external_functions.len() as u128)); // n_external_functions
    compiled_class_args.push(EndpointArg::Pointer(PointerArg::Composed(external_functions))); // external_functions

    // L1 handlers
    let l1_handlers: Vec<EndpointArg> = casm_contract_class
        .entry_points_by_type
        .l1_handler
        .iter()
        .map(|ep| {
            let mut entry_point_args = Vec::new();

            // selector
            entry_point_args.push(EndpointArg::from(Felt::from(&ep.selector)));

            // offset
            entry_point_args.push(EndpointArg::from(ep.offset as u128));

            // n_builtins
            entry_point_args.push(EndpointArg::from(ep.builtins.len() as u128));

            // builtin_list
            let builtins: Vec<Felt> = ep
                .builtins
                .iter()
                .map(|builtin| Felt::from_bytes_be_slice(builtin.as_bytes()))
                .collect();
            entry_point_args.push(EndpointArg::Pointer(PointerArg::Array(builtins)));

            EndpointArg::Pointer(PointerArg::Composed(entry_point_args))
        })
        .collect();

    compiled_class_args.push(EndpointArg::from(l1_handlers.len() as u128)); // n_l1_handlers
    compiled_class_args.push(EndpointArg::Pointer(PointerArg::Composed(l1_handlers))); // l1_handlers

    // Constructors
    let constructors: Vec<EndpointArg> = casm_contract_class
        .entry_points_by_type
        .constructor
        .iter()
        .map(|ep| {
            let mut entry_point_args = Vec::new();

            // selector
            entry_point_args.push(EndpointArg::from(Felt::from(&ep.selector)));

            // offset
            entry_point_args.push(EndpointArg::from(ep.offset as u128));

            // n_builtins
            entry_point_args.push(EndpointArg::from(ep.builtins.len() as u128));

            // builtin_list
            let builtins: Vec<Felt> = ep
                .builtins
                .iter()
                .map(|builtin| Felt::from_bytes_be_slice(builtin.as_bytes()))
                .collect();
            entry_point_args.push(EndpointArg::Pointer(PointerArg::Array(builtins)));

            EndpointArg::Pointer(PointerArg::Composed(entry_point_args))
        })
        .collect();

    compiled_class_args.push(EndpointArg::from(constructors.len() as u128)); // n_constructors
    compiled_class_args.push(EndpointArg::Pointer(PointerArg::Composed(constructors))); // constructors

    // Bytecode - convert from BigUintAsHex to Felt
    let bytecode: Vec<Felt> = casm_contract_class
        .bytecode
        .iter()
        .map(|big_uint_hex| Felt::from(&big_uint_hex.value))
        .collect();

    compiled_class_args.push(EndpointArg::from(bytecode.len() as u128)); // bytecode_length
    compiled_class_args.push(EndpointArg::Pointer(PointerArg::Array(bytecode))); // bytecode_ptr

    vec![EndpointArg::Pointer(PointerArg::Composed(compiled_class_args))]
}

#[test]
fn test_compiled_class_hash_blake() -> Cairo0EntryPointRunnerResult<()> {
    // Set up the entry point runner configuration
    let runner_config = EntryPointRunnerConfig {
        layout: LayoutName::all_cairo,
        trace_enabled: false,
        verify_secure: false,
        proof_mode: false,
        add_main_prefix_to_entrypoint: false, // Set to false since we're using full path
    };

    // Create the compiled class argument from test contract data
    let explicit_args = create_compiled_class_from_test_contract();

    // Set up implicit arguments (range_check_ptr for Blake2s)
    let implicit_args = vec![ImplicitArg::Builtin(BuiltinName::range_check)];

    // Expected return value (the hash as a felt)
    let expected_return_values = vec![
        EndpointArg::from(Felt::ZERO), // We expect a felt hash return value
    ];

    // Get the OS program as a string (it contains the compiled Blake2s hash function)
    let program_str = std::str::from_utf8(apollo_starknet_os_program::OS_PROGRAM_BYTES)
        .expect("OS program should be valid UTF-8");

    // Set up hint locals (empty for now)
    let hint_locals: HashMap<String, Box<dyn std::any::Any>> = HashMap::new();

    // Use the Blake2s version
    let (implicit_return_values, explicit_return_values, _runner) = run_cairo_0_entry_point(
        &runner_config,
        program_str,
        "starkware.starknet.core.os.contract_class.blake_compiled_class_hash.compiled_class_hash",
        &explicit_args,
        &implicit_args,
        &expected_return_values,
        hint_locals,
    )?;

    // Verify we got a return value
    assert_eq!(explicit_return_values.len(), 1, "Expected exactly one return value");
    assert!(!implicit_return_values.is_empty(), "Expected implicit return values");

    // The return value should be a felt (the computed hash)
    match &explicit_return_values[0] {
        EndpointArg::Value(ValueArg::Single(hash)) => {
            println!("Computed Blake2s compiled class hash: {}", hash);
            // Verify the hash is not zero (a basic sanity check)
            assert_ne!(*hash, Felt::ZERO, "Hash should not be zero");
        }
        _ => panic!("Expected a single felt return value"),
    }

    Ok(())
}
