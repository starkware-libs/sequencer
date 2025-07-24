use std::collections::HashMap;

use apollo_starknet_os_program::test_programs::BLAKE_COMPILED_CLASS_HASH_BYTES;
use blockifier::execution::execution_utils::blake_hash_execution_resources;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::vm::runners::cairo_runner::CairoRunner;
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

// TODO(AvivG): Add test for gas usage estimation (including blake opcode evaluation).
fn validate_estimation(cairo_runner: &CairoRunner, test_data: &[Felt]) {
    // TODO(Aviv): Use `Blake2Felt252::SMALL_THRESHOLD` when exposed.
    const SMALL_THRESHOLD: Felt = Felt::from_hex_unchecked("8000000000000000"); // 2^63

    let (n_small_felts, n_big_felts) = test_data.iter().fold((0, 0), |(small, big), felt| {
        if *felt >= SMALL_THRESHOLD { (small, big + 1) } else { (small + 1, big) }
    });

    let expected_resources = blake_hash_execution_resources(n_big_felts, n_small_felts);

    // TODO(AvivG): Investigate the 6-step discrepancy.
    let expected_steps = expected_resources.n_steps - 6;
    let actual_steps = cairo_runner.vm.get_current_step();
    assert_eq!(actual_steps, expected_steps);

    // TODO(AvivG): Investigate the +3 discrepancy.
    let mut expected_builtins = expected_resources.builtin_instance_counter.clone();
    expected_builtins
        .insert(BuiltinName::range_check, expected_builtins[&BuiltinName::range_check] + 3);

    // All other builtins should have zero usage
    for builtin in &cairo_runner.vm.builtin_runners {
        let actual_usage = builtin.get_used_instances(&cairo_runner.vm.segments).unwrap();
        let expected_usage = expected_builtins[&builtin.name()];

        assert_eq!(
            actual_usage,
            expected_usage,
            "{:?} builtin usage mismatch. Actual: {}, Expected: {}",
            builtin.name(),
            actual_usage,
            expected_usage,
        );
    }
}

/// Test that compares Cairo and Rust implementations of
/// encode_felt252_data_and_calc_blake_hash.
#[rstest]
// TODO(AvivG): Add the empty case once the cairo implementation supports it.
// #[case::empty(vec![])]
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

            validate_estimation(&cairo_runner, &test_data);
        }
        Err(e) => {
            panic!("Failed to run Cairo blake2s function: {e:?}");
        }
    }
}
