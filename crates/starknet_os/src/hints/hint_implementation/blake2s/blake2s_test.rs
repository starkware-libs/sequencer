use std::collections::HashMap;

use blockifier::execution::casm_hash_estimation::expected::{
    BASE_STEPS_FULL_MSG_EXPECT,
    BASE_STEPS_PARTIAL_MSG_EXPECT,
    STEPS_DISCOUNT_PER_FULL_MSG_EXPECT,
    STEPS_EMPTY_INPUT_EXPECT,
    STEPS_PER_LARGE_FELT_EXPECT,
    STEPS_PER_SMALL_FELT_EXPECT,
};
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

use crate::hints::hint_implementation::state_diff_encryption::utils::calc_blake_hash;
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

    estimated.vm_resources.clone()
}

/// Returns the result and the resources used.
fn cairo_encode_felt252_data_and_calc_blake_hash(input: &[Felt]) -> (Felt, ExecutionResources) {
    let runner_config = EntryPointRunnerConfig {
        layout: LayoutName::all_cairo,
        trace_enabled: false,
        verify_secure: false,
        proof_mode: false,
        add_main_prefix_to_entrypoint: false,
        validate_builtins_offset: true,
    };

    let data_len = input.len();
    let explicit_args = vec![
        EndpointArg::from(Felt::from(data_len)),
        EndpointArg::Pointer(PointerArg::Array(
            input.iter().map(|felt| MaybeRelocatable::Int(*felt)).collect(),
        )),
    ];
    let implicit_args = vec![ImplicitArg::Builtin(BuiltinName::range_check)];
    let expected_return_values = vec![EndpointArg::from(Felt::ZERO)];
    let hint_locals: HashMap<String, Box<dyn std::any::Any>> = HashMap::new();

    // Call the Cairo entrypoint.
    // This entrypoint does not use state reader.
    let state_reader = None;
    let (_, explicit_return_values, cairo_runner) = initialize_and_run_cairo_0_entry_point(
        &runner_config,
        apollo_starknet_os_program::OS_PROGRAM_BYTES,
        "starkware.cairo.common.cairo_blake2s.blake2s.encode_felt252_data_and_calc_blake_hash",
        &explicit_args,
        &implicit_args,
        &expected_return_values,
        hint_locals,
        state_reader,
    )
    .unwrap_or_else(|e| panic!("Failed to run Cairo blake2s function: {e:?}"));

    assert_eq!(explicit_return_values.len(), 1, "Expected exactly one return value");

    let EndpointArg::Value(ValueArg::Single(MaybeRelocatable::Int(cairo_hash_felt))) =
        &explicit_return_values[0]
    else {
        panic!("Expected a single felt return value");
    };
    (*cairo_hash_felt, cairo_runner.get_execution_resources().unwrap().filter_unused_builtins())
}

fn cairo_encode_felt252_data_and_calc_blake_hash_steps(input: &[Felt]) -> usize {
    cairo_encode_felt252_data_and_calc_blake_hash(input).1.n_steps
}

/// Asserts the estimated constants correspond to empiric measurements.
#[rstest]
fn test_blake_step_constants() {
    let large_felt = Blake2Felt252::SMALL_THRESHOLD;
    let small_felt = Blake2Felt252::SMALL_THRESHOLD - Felt::ONE;
    const LARGE_FELTS_PER_MESSAGE: usize = CasmV2HashResourceEstimate::U32_WORDS_PER_MESSAGE
        / CasmV2HashResourceEstimate::U32_WORDS_PER_LARGE_FELT;

    // Test empty input.
    let steps_empty = cairo_encode_felt252_data_and_calc_blake_hash_steps(&[]);
    STEPS_EMPTY_INPUT_EXPECT.assert_eq(&steps_empty.to_string());

    // Small felt overhead (assuming the remainder).
    let one_small_felt_cost = cairo_encode_felt252_data_and_calc_blake_hash_steps(&[small_felt]);
    let two_small_felts_cost =
        cairo_encode_felt252_data_and_calc_blake_hash_steps(&[small_felt; 2]);
    let small_felt_overhead = two_small_felts_cost - one_small_felt_cost;
    STEPS_PER_SMALL_FELT_EXPECT.assert_eq(&small_felt_overhead.to_string());

    // Base cost for partial message.
    let base_partial_message_cost = one_small_felt_cost - small_felt_overhead;
    BASE_STEPS_PARTIAL_MSG_EXPECT.assert_eq(&base_partial_message_cost.to_string());

    // Large felt overhead.
    let one_large_felt_cost = cairo_encode_felt252_data_and_calc_blake_hash_steps(&[large_felt]);
    let large_felt_overhead = one_large_felt_cost - base_partial_message_cost;
    STEPS_PER_LARGE_FELT_EXPECT.assert_eq(&large_felt_overhead.to_string());

    // Discount per full message.
    let full_message_large_felts_cost =
        cairo_encode_felt252_data_and_calc_blake_hash_steps(&[large_felt; LARGE_FELTS_PER_MESSAGE]);
    let two_full_messages_large_felts_cost = cairo_encode_felt252_data_and_calc_blake_hash_steps(
        &[large_felt; LARGE_FELTS_PER_MESSAGE * 2],
    );
    let discount_per_full_message = 2 * large_felt_overhead
        - (two_full_messages_large_felts_cost - full_message_large_felts_cost);
    STEPS_DISCOUNT_PER_FULL_MSG_EXPECT.assert_eq(&discount_per_full_message.to_string());

    // Base cost for input aligned to full messages.
    let base_full_message_cost =
        full_message_large_felts_cost - 2 * large_felt_overhead + discount_per_full_message;
    BASE_STEPS_FULL_MSG_EXPECT.assert_eq(&base_full_message_cost.to_string());
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
#[case::two_full_msgs(vec![Felt::from(1u64 << 63); 4])]
#[case::many_large(vec![Felt::from(1u64 << 63); 100])]
#[case::very_many_msgs(vec![Felt::from(1u64 << 63); 200])]
fn test_cairo_vs_rust_blake2s_implementation(#[case] test_data: Vec<Felt>) {
    let (cairo_hash_felt, actual_resources) =
        cairo_encode_felt252_data_and_calc_blake_hash(&test_data);
    let rust_hash = Blake2Felt252::encode_felt252_data_and_calc_blake_hash(&test_data);
    assert_eq!(
        rust_hash, cairo_hash_felt,
        "Blake2s hash mismatch: Rust={rust_hash}, Cairo={cairo_hash_felt}",
    );

    // TODO(AvivG): consider moving this to the where the estimate methods are defined.
    let estimated_resources = estimated_encode_and_blake_hash_execution_resources(&test_data);
    // Asserts that actual Cairo execution resources match the estimate.
    assert_eq!(actual_resources, estimated_resources);
}

/// Test that compares the Cairo0 `calc_naive_blake_hash` with its Rust equivalent.
// TODO(Yonatan): remove #[ignore] once calc_naive_blake_hash is used in the virtual OS program.
#[rstest]
#[case::empty(vec![])]
#[case::single_zero(vec![Felt::ZERO])]
#[case::single_one(vec![Felt::ONE])]
#[case::two_felts(vec![Felt::from(12), Felt::from(34)])]
#[case::many_felts(vec![Felt::from(7u64); 20])]
#[ignore]
fn test_calc_naive_blake_hash(#[case] test_data: Vec<Felt>) {
    let runner_config = EntryPointRunnerConfig {
        layout: LayoutName::all_cairo,
        trace_enabled: false,
        verify_secure: false,
        proof_mode: false,
        add_main_prefix_to_entrypoint: false,
        validate_builtins_offset: true,
    };

    let (_, return_values, _) = initialize_and_run_cairo_0_entry_point(
        &runner_config,
        apollo_starknet_os_program::VIRTUAL_OS_PROGRAM_BYTES,
        "starkware.starknet.core.os.naive_blake.calc_naive_blake_hash",
        &[
            EndpointArg::from(Felt::from(test_data.len())),
            EndpointArg::Pointer(PointerArg::Array(
                test_data.iter().map(|felt| MaybeRelocatable::Int(*felt)).collect(),
            )),
        ],
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        &[EndpointArg::from(Felt::ZERO)],
        HashMap::new(),
        None,
    )
    .unwrap();

    let [EndpointArg::Value(ValueArg::Single(MaybeRelocatable::Int(cairo_hash)))] =
        return_values.as_slice()
    else {
        panic!("Expected a single felt return value");
    };
    assert_eq!(calc_blake_hash(&test_data), *cairo_hash);
}
