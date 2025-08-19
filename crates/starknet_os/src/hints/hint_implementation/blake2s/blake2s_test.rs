use std::collections::HashMap;

use blake2s::encode_felt252_data_and_calc_blake_hash;
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

use crate::test_utils::cairo_runner::{
    initialize_and_run_cairo_0_entry_point,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
    ValueArg,
};

/// Counts the number of small and big felts in the data.
fn data_to_felt_count(data: &[Felt]) -> FeltSizeCount {
    // TODO(AvivG): Use `Blake2Felt252::SMALL_THRESHOLD` when exposed.
    const SMALL_THRESHOLD: Felt = Felt::from_hex_unchecked("8000000000000000"); // 2^63

    let felt_size_groups = FeltSizeCount { large: 0, small: 0 };

    data.iter().fold(felt_size_groups, |mut felt_size_groups, felt| {
        if *felt >= SMALL_THRESHOLD {
            felt_size_groups.large += 1;
        } else {
            felt_size_groups.small += 1;
        }
        felt_size_groups
    })
}

/// Return the estimated execution resources for Blake2s hashing.
fn estimated_encode_and_blake_hash_execution_resources(data: &[Felt]) -> ExecutionResources {
    let felt_size_groups = data_to_felt_count(data);
    let estimated =
        CasmV2HashResourceEstimate::estimated_resources_of_hash_function(&felt_size_groups);

    let mut resources = estimated.resources().clone();
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

    let rust_hash = encode_felt252_data_and_calc_blake_hash(&test_data);

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
