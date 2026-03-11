use std::collections::HashMap;

use blockifier::execution::casm_hash_estimation::{
    CasmV2HashResourceEstimate,
    EstimateCasmHashResources,
};
use blockifier::execution::contract_class::FeltSizeCount;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use rstest::rstest;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::Blake2Felt252;

use crate::hints::hint_implementation::state_diff_encryption::utils::{
    blake2s_to_felt,
    naive_encode_felts_to_u32s,
};
use crate::test_utils::cairo_runner::{
    initialize_and_run_cairo_0_entry_point,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
    ValueArg,
};

/// Return the estimated execution resources for Blake2s hashing.
fn estimated_encode_and_blake_hash_execution_resources(data: &[Felt]) -> ExecutionResources {
    let felt_size_groups = FeltSizeCount::from(data);
    let estimated =
        CasmV2HashResourceEstimate::estimated_resources_of_hash_function(&felt_size_groups);

    let mut resources = estimated.vm_resources.clone();
    resources.n_steps -= 1;

    resources
}

/// Test that compares Cairo and Rust implementations of
/// encode_felt252_data_and_calc_blake_hash.
#[rstest]
// TODO(Aviv): Add the empty case once the cairo implementation supports it.
#[case::empty(vec![])]
#[case::boundary_small_felt(vec![Felt::from((1u64 << 63) - 1)])]
#[case::boundary_at_2_63(vec![Felt::from(1u64 << 63)])]
#[case::very_large_felt(vec![Felt::from_hex("0x800000000000011000000000000000000000000000000000000000000000000").unwrap()])]
#[case::mixed_small_large(vec![Felt::from(42), Felt::from(1u64 << 63), Felt::from(1337)])]
#[case::many_large(vec![Felt::from(1u64 << 63); 100])]
fn test_cairo_vs_rust_blake2s_implementation(#[case] test_data: Vec<Felt>) {
    let runner_config = EntryPointRunnerConfig {
        layout: LayoutName::all_cairo,
        trace_enabled: false,
        verify_secure: false,
        proof_mode: false,
        add_main_prefix_to_entrypoint: false,
    };

    let rust_hash = Blake2Felt252::encode_felt252_data_and_calc_blake_hash(&test_data);

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
        apollo_starknet_os_program::OS_PROGRAM_BYTES,
        "starkware.cairo.common.cairo_blake2s.blake2s.encode_felt252_data_and_calc_blake_hash",
        &explicit_args,
        &implicit_args,
        &expected_return_values,
        hint_locals,
        state_reader,
    );

    match result {
        Ok((_, explicit_return_values, cairo_runner)) => {
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

            // TODO(AvivG): consider moving this to the where the estimate methods are defined.
            let actual_resources =
                cairo_runner.get_execution_resources().unwrap().filter_unused_builtins();
            let estimated_resources =
                estimated_encode_and_blake_hash_execution_resources(&test_data);
            // Asserts that actual Cairo execution resources match the estimate.
            assert_eq!(actual_resources, estimated_resources);
        }
        Err(e) => {
            panic!("Failed to run Cairo blake2s function: {e:?}");
        }
    }
}

/// Compute the Rust-side naive Blake2s hash: encode felts to u32 LE limbs, then Blake2s-256.
fn rust_naive_blake_hash(data: &[Felt]) -> Felt {
    let u32_words = naive_encode_felts_to_u32s(data.to_vec());
    let bytes: Vec<u8> = u32_words.iter().flat_map(|w| w.to_le_bytes()).collect();
    blake2s_to_felt(&bytes)
}

/// Test that compares the Cairo0 `calc_naive_blake_hash` with its Rust equivalent.
#[rstest]
#[case::single_zero(vec![Felt::ZERO])]
#[case::single_one(vec![Felt::ONE])]
#[case::boundary_at_2_63(vec![Felt::from(1u64 << 63)])]
#[case::two_felts(vec![Felt::from(12), Felt::from(34)])]
#[case::typical_message(vec![
    Felt::from(0x1234u64),
    Felt::from(85u64),
    Felt::from(2u64),
    Felt::from(12u64),
    Felt::from(34u64),
])]
#[case::many_felts(vec![Felt::from(7u64); 20])]
fn test_calc_naive_blake_hash(#[case] test_data: Vec<Felt>) {
    let runner_config = EntryPointRunnerConfig {
        layout: LayoutName::all_cairo,
        trace_enabled: false,
        verify_secure: false,
        proof_mode: false,
        add_main_prefix_to_entrypoint: false,
    };

    let rust_hash = rust_naive_blake_hash(&test_data);

    let explicit_args = vec![
        EndpointArg::from(Felt::from(test_data.len())),
        EndpointArg::Pointer(PointerArg::Array(
            test_data.iter().map(|felt| MaybeRelocatable::Int(*felt)).collect(),
        )),
    ];
    let implicit_args = vec![ImplicitArg::Builtin(BuiltinName::range_check)];
    let expected_return_values = vec![EndpointArg::from(Felt::ZERO)];

    let result = initialize_and_run_cairo_0_entry_point(
        &runner_config,
        apollo_starknet_os_program::VIRTUAL_OS_PROGRAM_BYTES,
        "starkware.starknet.core.os.naive_blake.calc_naive_blake_hash",
        &explicit_args,
        &implicit_args,
        &expected_return_values,
        HashMap::new(),
        None,
    );

    match result {
        Ok((_, explicit_return_values, _)) => {
            assert_eq!(explicit_return_values.len(), 1, "Expected exactly one return value");
            let EndpointArg::Value(ValueArg::Single(MaybeRelocatable::Int(cairo_hash))) =
                &explicit_return_values[0]
            else {
                panic!("Expected a single felt return value");
            };
            assert_eq!(
                rust_hash, *cairo_hash,
                "Naive Blake hash mismatch: Rust={rust_hash}, Cairo={cairo_hash}",
            );
        }
        Err(e) => {
            panic!("Failed to run Cairo calc_naive_blake_hash: {e:?}");
        }
    }
}
