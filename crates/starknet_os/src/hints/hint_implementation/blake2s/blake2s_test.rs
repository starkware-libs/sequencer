use std::collections::HashMap;
use std::env::current_dir;
use std::fs::File;
use std::io::Read;

use apollo_starknet_os_program::OS_PROGRAM_BYTES;
use blake2s::encode_felt252_data_and_calc_224_bit_blake_hash;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_vm::any_box;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use crate::hints::hint_implementation::compiled_class::utils::create_bytecode_segment_structure;
use crate::test_utils::cairo_runner::{
    initialize_and_run_cairo_0_entry_point,
    initialize_cairo_runner,
    run_cairo_0_entrypoint,
    Cairo0EntryPointRunnerResult,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
    ValueArg,
};
use crate::vm_utils::LoadCairoObject;

/// Test that compares Cairo and Rust implementations of
/// encode_felt252_data_and_calc_224_bit_blake_hash.
#[rstest]
#[case::empty(vec![])]
#[case::boundary_under_2_63(vec![Felt::from((1u64 << 63) - 1)])]
#[case::boundary_at_2_63(vec![Felt::from(1u64 << 63)])]
#[case::very_large_felt(vec![Felt::from_hex("0x800000000000011000000000000000000000000000000000000000000000000").unwrap()])]
#[case::many_felts(
    vec![
        Felt::from(1),
        Felt::from(2),
        Felt::from(3),
        Felt::from(4),
        Felt::from(5),
        Felt::from(6),
        Felt::from(7),
        Felt::from(8),
        Felt::from(9),
        Felt::from(10),
        Felt::from(11),
        Felt::from(12),
        Felt::from(13),
        Felt::from(14),
        Felt::from(15),
    ]
)]
#[case::mixed_small_large(vec![Felt::from(42), Felt::from(1u64 << 63), Felt::from(1337)])]
#[case::max_u64(vec![Felt::from(u64::MAX)])]
fn test_cairo_vs_rust_blake2s_implementation(#[case] test_data: Vec<Felt>) {
    let runner_config = EntryPointRunnerConfig {
        layout: LayoutName::all_cairo,
        trace_enabled: false,
        verify_secure: false,
        proof_mode: false,
        add_main_prefix_to_entrypoint: false,
    };

    // Get the OS program as bytes
    let program_bytes = OS_PROGRAM_BYTES;

    // Calculate hash using Rust implementation
    let rust_hash = encode_felt252_data_and_calc_224_bit_blake_hash(&test_data);

    // Calculate hash using Cairo implementation
    let data_len = test_data.len();
    let explicit_args = vec![
        EndpointArg::from(Felt::from(data_len)), // data_len
        EndpointArg::Pointer(PointerArg::Array(
            test_data.iter().map(|&felt| MaybeRelocatable::from(felt)).collect(),
        )), // data array
    ];

    let implicit_args = vec![ImplicitArg::Builtin(BuiltinName::range_check)];

    let expected_return_values = vec![EndpointArg::from(Felt::ZERO)]; // Placeholder

    let hint_locals: HashMap<String, Box<dyn std::any::Any>> = HashMap::new();

    // Call the Cairo function
    let result = initialize_and_run_cairo_0_entry_point(
        &runner_config,
        program_bytes,
        "starkware.starknet.core.os.hash.blake2s.encode_felt252_data_and_calc_blake_hash",
        &explicit_args,
        &implicit_args,
        &expected_return_values,
        hint_locals,
        None, // state_reader
    );

    match result {
        Ok((_explicit_return_values, implicit_return_values, _)) => {
            assert_eq!(
                implicit_return_values.len(),
                1,
                "Expected exactly one implicit return value"
            );

            match &implicit_return_values[0] {
                EndpointArg::Value(ValueArg::Single(cairo_hash)) => {
                    if let MaybeRelocatable::Int(cairo_hash_felt) = cairo_hash {
                        println!("Rust hash: {}, Cairo hash: {}", rust_hash, cairo_hash_felt);
                        assert_eq!(
                            rust_hash, *cairo_hash_felt,
                            "Blake2s hash mismatch: Rust={}, Cairo={}",
                            rust_hash, cairo_hash_felt
                        );
                    } else {
                        panic!("Expected an integer value, got relocatable");
                    }
                }
                _ => panic!("Expected a single felt return value"),
            }
        }
        Err(e) => {
            panic!("Failed to run Cairo blake2s function: {:?}", e);
        }
    }
}

/// Creates a CompiledClass structure from the test contract JSON data using
/// cairo_lang_starknet_classes
fn create_compiled_class_from_test_contract() -> CasmContractClass {
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
    casm_contract_class
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

    // Set up implicit arguments (range_check_ptr for Blake2s)
    let implicit_args = vec![ImplicitArg::Builtin(BuiltinName::range_check)];

    // Expected return value (the hash as a felt)
    let expected_return_values = vec![
        EndpointArg::from(Felt::ZERO), // We expect a felt hash return value
    ];

    // Get the OS program as bytes (not string)
    let program_bytes = apollo_starknet_os_program::OS_PROGRAM_BYTES;

    let contract_class = create_compiled_class_from_test_contract();
    // Set up hint locals (empty for now)
    let mut hint_locals: HashMap<String, Box<dyn std::any::Any>> = HashMap::new();
    let bytecode_structure = create_bytecode_segment_structure(
        &contract_class.bytecode.iter().map(|x| Felt::from(&x.value)).collect::<Vec<_>>(),
        contract_class.get_bytecode_segment_lengths(),
    ).unwrap();
    hint_locals.insert("bytecode_segment_structure".to_string(), any_box!(bytecode_structure));

    // Use the Blake2s version
    let (mut runner, program, entrypoint) = initialize_cairo_runner(
        &runner_config,
        program_bytes,
        "starkware.starknet.core.os.contract_class.blake_compiled_class_hash.blake_compiled_class_hash",
        &implicit_args,
        hint_locals,
    )?;

    // Create constants with String keys instead of &str
    let constants = HashMap::from([(
        "starkware.starknet.core.os.contract_class.compiled_class_struct.COMPILED_CLASS_VERSION"
            .to_string(),
        Felt::from_bytes_be_slice(b"COMPILED_CLASS_V1"),
    )]);

    let contract_class_base = runner.vm.add_memory_segment();

    // Use the program object instead of program_bytes for load_into
    contract_class.load_into(&mut runner.vm, &program, contract_class_base, &constants).unwrap();

    let explicit_args = vec![EndpointArg::Value(ValueArg::Single(contract_class_base.into()))];
    // run_cairo_0_entrypoint returns 2 values, not 3, and takes a mutable runner
    let (implicit_return_values, explicit_return_values) = run_cairo_0_entrypoint(
        entrypoint,
        &explicit_args,
        &implicit_args,
        None,
        &mut runner,
        &program,
        &runner_config,
        &expected_return_values,
    ).unwrap();

    // Verify we got a return value
    assert_eq!(explicit_return_values.len(), 1, "Expected exactly one return value");
    assert!(!implicit_return_values.is_empty(), "Expected implicit return values");

    // The return value should be a felt (the computed hash)
    match &explicit_return_values[0] {
        EndpointArg::Value(ValueArg::Single(hash)) => {
            if let MaybeRelocatable::Int(hash_felt) = hash {
                println!("Computed Blake2s compiled class hash: {}", hash_felt);
                // Verify the hash is not zero (a basic sanity check)
                // Compare hash_felt directly (using cairo_vm::Felt252 comparison)
                assert_ne!(hash_felt, &cairo_vm::Felt252::ZERO, "Hash should not be zero");
            } else {
                panic!("Expected an integer value, got relocatable");
            }
        }
        _ => panic!("Expected a single felt return value"),
    }
    Ok(())
}

#[test]
fn test_compiled_class_hash_poseidon() -> Cairo0EntryPointRunnerResult<()> {
    // Set up the entry point runner configuration
    let runner_config = EntryPointRunnerConfig {
        layout: LayoutName::all_cairo,
        trace_enabled: false,
        verify_secure: false,
        proof_mode: false,
        add_main_prefix_to_entrypoint: false, // Set to false since we're using full path
    };

    // Set up implicit arguments (range_check_ptr for Blake2s)
    let implicit_args = vec![ImplicitArg::Builtin(BuiltinName::range_check), ImplicitArg::Builtin(BuiltinName::poseidon)];

    // Expected return value (the hash as a felt)
    let expected_return_values = vec![
        EndpointArg::from(Felt::ZERO), // We expect a felt hash return value
    ];

    // Get the OS program as bytes (not string)
    let program_bytes = apollo_starknet_os_program::OS_PROGRAM_BYTES;

    let contract_class = create_compiled_class_from_test_contract();
    // Set up hint locals (empty for now)
    let mut hint_locals: HashMap<String, Box<dyn std::any::Any>> = HashMap::new();
    let bytecode_structure = create_bytecode_segment_structure(
        &contract_class.bytecode.iter().map(|x| Felt::from(&x.value)).collect::<Vec<_>>(),
        contract_class.get_bytecode_segment_lengths(),
    ).unwrap();
    hint_locals.insert("bytecode_segment_structure".to_string(), any_box!(bytecode_structure));

    // Use the Blake2s version
    let (mut runner, program, entrypoint) = initialize_cairo_runner(
        &runner_config,
        program_bytes,
        "starkware.starknet.core.os.contract_class.poseidon_compiled_class_hash.poseidon_compiled_class_hash",
        &implicit_args,
        hint_locals,
    )?;

    // Create constants with String keys instead of &str
    let constants = HashMap::from([(
        "starkware.starknet.core.os.contract_class.compiled_class_struct.COMPILED_CLASS_VERSION"
            .to_string(),
        Felt::from_bytes_be_slice(b"COMPILED_CLASS_V1"),
    )]);

    let contract_class_base = runner.vm.add_memory_segment();

    // Use the program object instead of program_bytes for load_into
    contract_class.load_into(&mut runner.vm, &program, contract_class_base, &constants).unwrap();

    let explicit_args = vec![EndpointArg::Value(ValueArg::Single(contract_class_base.into()))];
    // run_cairo_0_entrypoint returns 2 values, not 3, and takes a mutable runner
    let (implicit_return_values, explicit_return_values) = run_cairo_0_entrypoint(
        entrypoint,
        &explicit_args,
        &implicit_args,
        None,
        &mut runner,
        &program,
        &runner_config,
        &expected_return_values,
    ).unwrap();

    // Verify we got a return value
    assert_eq!(explicit_return_values.len(), 1, "Expected exactly one return value");
    assert!(!implicit_return_values.is_empty(), "Expected implicit return values");

    // The return value should be a felt (the computed hash)
    match &explicit_return_values[0] {
        EndpointArg::Value(ValueArg::Single(hash)) => {
            if let MaybeRelocatable::Int(hash_felt) = hash {
                println!("Computed Poseidon compiled class hash: {}", hash_felt);
                // Verify the hash is not zero (a basic sanity check)
                // Compare hash_felt directly (using cairo_vm::Felt252 comparison)
                assert_ne!(hash_felt, &cairo_vm::Felt252::ZERO, "Hash should not be zero");
            } else {
                panic!("Expected an integer value, got relocatable");
            }
        }
        _ => panic!("Expected a single felt return value"),
    }
    Ok(())
}
