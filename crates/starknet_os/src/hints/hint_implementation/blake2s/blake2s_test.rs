use std::collections::HashMap;

use apollo_starknet_os_program::OS_PROGRAM_BYTES;
use blake2s::encode_felt252_data_and_calc_224_bit_blake_hash;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use crate::test_utils::cairo_runner::{
    initialize_and_run_cairo_0_entry_point,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
    ValueArg,
};

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
