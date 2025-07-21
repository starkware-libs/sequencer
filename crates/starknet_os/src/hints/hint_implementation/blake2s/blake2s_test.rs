use std::collections::HashMap;
use std::path::PathBuf;

use apollo_infra_utils::cairo0_compiler::compile_cairo0_program;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use rstest::rstest;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::Blake2Felt252;

use crate::test_utils::cairo_runner::{
    initialize_and_run_cairo_0_entry_point,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
    ValueArg,
};

/// Helper function to compile the blake_compiled_class_hash.cairo file.
fn compile_blake_hash_program() -> Vec<u8> {
    // Get the path to the cairo source files
    let cairo_root_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent() // Go up from starknet_os
        .unwrap()
        .join("apollo_starknet_os_program")
        .join("src")
        .join("cairo");

    let blake_file_path = cairo_root_path
        .join("starkware")
        .join("starknet")
        .join("core")
        .join("os")
        .join("contract_class")
        .join("blake_compiled_class_hash.cairo");

    compile_cairo0_program(blake_file_path, cairo_root_path)
        .expect("Failed to compile blake_compiled_class_hash.cairo")
}

/// Test that compares Cairo and Rust implementations of
/// encode_felt252_data_and_calc_blake_hash.
#[rstest]
#[case::empty(vec![])]
#[case::boundary_under_2_63(vec![Felt::from((1u64 << 63) - 1)])]
#[case::boundary_at_2_63(vec![Felt::from(1u64 << 63)])]
#[case::very_large_felt(vec![Felt::from_hex("0x800000000000011000000000000000000000000000000000000000000000000").unwrap()])]
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

    // Compile the blake hash program containing the function we need
    let program_bytes = compile_blake_hash_program();

    let rust_hash = Blake2Felt252::encode_felt252_data_and_calc_224_bit_blake_hash(&test_data);

    let data_len = test_data.len();
    let explicit_args = vec![
        EndpointArg::from(Felt::from(data_len)),
        EndpointArg::Pointer(PointerArg::Array(
            test_data.iter().map(|felt| MaybeRelocatable::Int(*felt)).collect(),
        )),
    ];

    let implicit_args = vec![ImplicitArg::Builtin(BuiltinName::range_check)];

    let expected_return_values = vec![EndpointArg::from(Felt::ZERO)]; // Placeholder

    let hint_locals: HashMap<String, Box<dyn std::any::Any>> = HashMap::new();

    // Call the Cairo function
    let result = initialize_and_run_cairo_0_entry_point(
        &runner_config,
        &program_bytes,
        "starkware.cairo.common.cairo_blake2s.blake2s.\
         encode_felt252_data_and_calc_224_bit_blake_hash",
        &explicit_args,
        &implicit_args,
        &expected_return_values,
        hint_locals,
        None, // state_reader
    );

    match result {
        Ok((_, explicit_return_values, _)) => {
            assert_eq!(explicit_return_values.len(), 1, "Expected exactly one return value");

            match &explicit_return_values[0] {
                EndpointArg::Value(ValueArg::Single(cairo_hash)) => {
                    if let MaybeRelocatable::Int(cairo_hash_felt) = cairo_hash {
                        assert_eq!(
                            rust_hash, *cairo_hash_felt,
                            "Blake2s hash mismatch: Rust={}, Cairo={}",
                            rust_hash, cairo_hash_felt
                        );
                        println!("{}", format!("Blake2s hash mismatch: Rust={}, Cairo={}", rust_hash, cairo_hash_felt));
                    } else {
                        panic!("Expected an integer value, got a relocatable");
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
