use std::collections::HashMap;

use apollo_starknet_os_program::test_programs::BLAKE_COMPILED_CLASS_HASH_BYTES;
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

/// Test that compares Cairo and Rust implementations of
/// encode_felt252_data_and_calc_blake_hash.
#[rstest]
#[case::empty(vec![])]
#[case::boundary_small_felt(vec![Felt::from((1u64 << 63) - 1)])]
#[case::boundary_at_2_63(vec![Felt::from(1u64 << 63)])]
#[case::very_large_felt(vec![Felt::from_hex("0x800000000000011000000000000000000000000000000000000000000000000").unwrap()])]
#[case::mixed_small_large(vec![Felt::from(42), Felt::from(1u64 << 63), Felt::from(1337)])]
fn test_cairo_vs_rust_blake2s_implementation(#[case] test_data: Vec<Felt>) {
    let runner_config = EntryPointRunnerConfig {
        layout: LayoutName::all_cairo,
        trace_enabled: false,
        verify_secure: false,
        proof_mode: false,
        add_main_prefix_to_entrypoint: false,
    };

    let rust_hash = Blake2Felt252::encode_felt252_data_and_calc_224_bit_blake_hash(&test_data);

    let data_len = test_data.len();
    let explicit_args = vec![
        EndpointArg::from(Felt::from(data_len)),
        EndpointArg::Pointer(PointerArg::Array(
            test_data.iter().map(|felt| MaybeRelocatable::Int(*felt)).collect(),
        )),
    ];

    let implicit_args = vec![ImplicitArg::Builtin(BuiltinName::range_check)];

    let expected_return_values = vec![EndpointArg::from(Felt::ZERO)];

    let hint_locals: HashMap<String, Box<dyn std::any::Any>> = HashMap::new();

    // Call the Cairo entrypoint.
    // This entrypoint does not use state reader.
    let state_reader = None;
    let result = initialize_and_run_cairo_0_entry_point(
        &runner_config,
        BLAKE_COMPILED_CLASS_HASH_BYTES,
        "starkware.cairo.common.cairo_blake2s.blake2s.\
         encode_felt252_data_and_calc_224_bit_blake_hash",
        &explicit_args,
        &implicit_args,
        &expected_return_values,
        hint_locals,
        state_reader,
    );

    match result {
        Ok((_, explicit_return_values, _)) => {
            assert_eq!(explicit_return_values.len(), 1, "Expected exactly one return value");

            let EndpointArg::Value(ValueArg::Single(MaybeRelocatable::Int(cairo_hash_felt))) =
                &explicit_return_values[0]
            else {
                panic!("Expected a single felt return value");
            };
            assert_eq!(
                rust_hash, *cairo_hash_felt,
                "Blake2s hash mismatch: Rust={rust_hash}, Cairo={cairo_hash_felt}",
            );
        }
        Err(e) => {
            panic!("Failed to run Cairo blake2s function: {e:?}");
        }
    }
}
