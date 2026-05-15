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

    let mut resources = estimated.vm_resources.clone();
    resources.n_steps -= 1;

    resources
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
/// Note that the resulting constants cannot be used to compute the precise number of steps required
/// for any given input. As an example, the total steps required for input [x; n] where x is a small
/// felt and n ranges from 0 to 9 is: [169, 212, 230, 248, 266, 284, 302, 320, 336, 332]. The first
/// element is the overhead of the empty input; the 7 diffs between consecutive computations are
/// [43, 18, 18, 18, 18, 18, 18, 16, -4]. After the first two elements, we observe a periodic
/// overhead when adding small felts: 6 added small felts give an increase of 18 steps, the next
/// adds 16, and adding an 8th small felt actually *decreases the total by 4 steps.
#[rstest]
fn test_blake_step_constants() {
    let large_felt = Blake2Felt252::SMALL_THRESHOLD;
    let small_felt = Blake2Felt252::SMALL_THRESHOLD - Felt::ONE;
    const LARGE_FELTS_PER_MESSAGE: usize = CasmV2HashResourceEstimate::U32_WORDS_PER_MESSAGE
        / CasmV2HashResourceEstimate::U32_WORDS_PER_LARGE_FELT;
    const SMALL_FELTS_PER_MESSAGE: usize = CasmV2HashResourceEstimate::U32_WORDS_PER_MESSAGE
        / CasmV2HashResourceEstimate::U32_WORDS_PER_SMALL_FELT;

    // Test empty input.
    let steps_empty = cairo_encode_felt252_data_and_calc_blake_hash_steps(&[]);
    assert_eq!(steps_empty, CasmV2HashResourceEstimate::STEPS_EMPTY_INPUT);

    // Start with a baseline of one full word.
    let one_message_large_felts = vec![large_felt; LARGE_FELTS_PER_MESSAGE];
    let baseline_steps =
        cairo_encode_felt252_data_and_calc_blake_hash_steps(&one_message_large_felts);

    // Add another full word of large felts to compute the overhead per large felt.
    let large_felt_overhead = (cairo_encode_felt252_data_and_calc_blake_hash_steps(
        &one_message_large_felts
            .clone()
            .into_iter()
            .chain(one_message_large_felts.clone().into_iter())
            .collect::<Vec<Felt>>(),
    ) - baseline_steps)
        / LARGE_FELTS_PER_MESSAGE;
    assert_eq!(large_felt_overhead, CasmV2HashResourceEstimate::STEPS_PER_LARGE_FELT);

    // Add another full word of small felts to compute the overhead per small felt.
    let one_message_small_felts = vec![small_felt; SMALL_FELTS_PER_MESSAGE];
    let small_felt_overhead = (cairo_encode_felt252_data_and_calc_blake_hash_steps(
        &one_message_large_felts
            .clone()
            .into_iter()
            .chain(one_message_small_felts.clone().into_iter())
            .collect::<Vec<Felt>>(),
    ) - baseline_steps)
        / SMALL_FELTS_PER_MESSAGE;
    assert_eq!(small_felt_overhead, CasmV2HashResourceEstimate::STEPS_PER_SMALL_FELT);

    // Compute the full-word overhead by subtracting the overhead of one word of large felts, from
    // the result of exactly one word of large felts.
    let full_word_overhead = baseline_steps - (large_felt_overhead * LARGE_FELTS_PER_MESSAGE);
    assert_eq!(full_word_overhead, CasmV2HashResourceEstimate::BASE_STEPS_FULL_MSG);

    // Compute the two-word partial overhead by computing:
    // X, one full message of large felts + one half-message of small felts.
    // Y, one full message of large felts + one half-message of small felts + one more small felt.
    // Computing Y-X gives the overhead of the additional small felt plus the overhead of a 2-word
    // remainder.
    let one_plus_half_message_steps = cairo_encode_felt252_data_and_calc_blake_hash_steps(
        &one_message_large_felts
            .clone()
            .into_iter()
            .chain(vec![small_felt; SMALL_FELTS_PER_MESSAGE / 2].into_iter())
            .collect::<Vec<Felt>>(),
    );
    let one_plus_half_message_plus_small_felt_steps =
        cairo_encode_felt252_data_and_calc_blake_hash_steps(
            &one_message_large_felts
                .clone()
                .into_iter()
                .chain(vec![small_felt; 1 + SMALL_FELTS_PER_MESSAGE / 2].into_iter())
                .collect::<Vec<Felt>>(),
        );
    let remainder = one_plus_half_message_plus_small_felt_steps
        - one_plus_half_message_steps
        - small_felt_overhead;
    assert_eq!(remainder, CasmV2HashResourceEstimate::STEPS_PER_2_U32_REMINDER);

    // Reuse the above computation to deduce the constant overhead for the case where the input does
    // not fit into an exact multiple of messages.
    let partial_message_overhead = one_plus_half_message_steps
        // Partial message: small felt overhead.
        - (SMALL_FELTS_PER_MESSAGE / 2) * CasmV2HashResourceEstimate::STEPS_PER_SMALL_FELT
        // Partial message: remainder per word-pair in the partial message.
        - (SMALL_FELTS_PER_MESSAGE / 2) * CasmV2HashResourceEstimate::STEPS_PER_2_U32_REMINDER
        // Overhead of first (full) message: two large felts.
        - LARGE_FELTS_PER_MESSAGE * CasmV2HashResourceEstimate::STEPS_PER_LARGE_FELT;
    assert_eq!(partial_message_overhead, CasmV2HashResourceEstimate::BASE_STEPS_PARTIAL_MSG);
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
