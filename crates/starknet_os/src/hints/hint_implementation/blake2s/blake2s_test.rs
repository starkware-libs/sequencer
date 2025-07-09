use std::collections::HashMap;

use apollo_starknet_os_program::OS_PROGRAM_BYTES;
use blake2s::encode_felt252_data_and_calc_224_bit_blake_hash;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use crate::test_utils::cairo_runner::{
    run_cairo_0_entry_point,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
    ValueArg,
};

/// Test that compares Cairo and Rust implementations of
/// encode_felt252_data_and_calc_224_bit_blake_hash.
#[rstest]
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

    // Get the OS program as bytes
    let program_bytes = OS_PROGRAM_BYTES;

    println!("Testing case: {:?}", test_data);

    // Calculate hash using Rust implementation
    let rust_hash = encode_felt252_data_and_calc_224_bit_blake_hash(&test_data);

    // Calculate hash using Cairo implementation
    let data_len = test_data.len();
    let explicit_args = vec![
        EndpointArg::from(Felt::from(data_len)), // data_len
        EndpointArg::Pointer(PointerArg::Array(test_data.clone())), // data array
    ];

    let implicit_args = vec![ImplicitArg::Builtin(BuiltinName::range_check)];

    let expected_return_values = vec![EndpointArg::from(Felt::ZERO)]; // Placeholder

    let hint_locals: HashMap<String, Box<dyn std::any::Any>> = HashMap::new();

    // Call the Cairo function
    let result = run_cairo_0_entry_point(
        &runner_config,
        program_bytes,
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
                    println!("Rust hash: {}, Cairo hash: {}", rust_hash, cairo_hash);
                    assert_eq!(
                        rust_hash, *cairo_hash,
                        "Blake2s hash mismatch: Rust={}, Cairo={}",
                        rust_hash, cairo_hash
                    );
                }
                _ => panic!("Expected a single felt return value"),
            }
        }
        Err(e) => {
            panic!("Failed to run Cairo blake2s function: {:?}", e);
        }
    }

    println!("✅ Test case passed! Cairo and Rust Blake2s implementations match.");
}
