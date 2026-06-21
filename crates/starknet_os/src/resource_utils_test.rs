use std::collections::HashMap;

use apollo_starknet_os_program::OS_PROGRAM_BYTES;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use rand::distr::{Distribution, StandardUniform};
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use rstest::rstest;
use starknet_types_core::felt::Felt;

use crate::test_utils::cairo_runner::{
    initialize_cairo_runner,
    run_cairo_0_entrypoint,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    ValueArg,
};
use crate::test_utils::{
    SHA256_BATCH_RESOURCES_CONSTANT,
    SHA256_BATCH_RESOURCES_LINEAR,
    SHA256_BATCH_SIZE,
    SHA512_BATCH_RESOURCES_CONSTANT,
    SHA512_BATCH_RESOURCES_LINEAR,
    SHA512_BATCH_SIZE,
};

/// SHA-256 block compression: takes 8 u32 state words and 16 u32 message words, returns the
/// new 8 u32 state. Wraps `sha2::compress256` (message words are big-endian per the SHA-256 spec).
fn sha_256_update_state(state: &[u32; 8], message: &[u32; 16]) -> [u32; 8] {
    let block = sha2::digest::generic_array::GenericArray::from_exact_iter(
        message.iter().flat_map(|word| word.to_be_bytes()),
    )
    .expect("message is exactly 64 bytes");
    let mut new_state = *state;
    sha2::compress256(&mut new_state, &[block]);
    new_state
}

/// SHA-512 block compression: takes 8 u64 state words and 16 u64 message words, returns the
/// new 8 u64 state. Wraps `sha2::compress512` (message words are big-endian per the SHA-512 spec).
fn sha_512_update_state(state: &[u64; 8], message: &[u64; 16]) -> [u64; 8] {
    let block = sha2::digest::generic_array::GenericArray::from_exact_iter(
        message.iter().flat_map(|word| word.to_be_bytes()),
    )
    .expect("message is exactly 128 bytes");
    let mut new_state = *state;
    sha2::compress512(&mut new_state, &[block]);
    new_state
}

/// Use T=u32 for sha256, T=u64 for sha512.
fn run_finalize_sha<T>(
    number_of_blocks: usize,
    cairo_finalize_fn: &str,
    sha_update_state_fn: fn(&[T; 8], &[T; 16]) -> [T; 8],
) -> ExecutionResources
where
    T: Clone + Copy,
    StandardUniform: Distribution<T> + Distribution<[T; 8]> + Distribution<[T; 16]>,
    Felt: From<T>,
{
    // Build the SHA instance array. Each instance is 32 felts:
    // [message (16) | initial_state (8) | output_state (8)].
    let mut rng = StdRng::seed_from_u64(42);
    let mut input: Vec<T> = Vec::new();
    for _ in 0..number_of_blocks {
        let message: [T; 16] = rng.random();
        let state: [T; 8] = rng.random();
        input.extend_from_slice(&message);
        input.extend_from_slice(&state);
        let output_state = sha_update_state_fn(&state, &message);
        input.extend_from_slice(&output_state);
    }

    let runner_config = EntryPointRunnerConfig {
        layout: LayoutName::starknet,
        add_main_prefix_to_entrypoint: false,
        ..Default::default()
    };
    let implicit_args = [
        ImplicitArg::Builtin(BuiltinName::range_check),
        ImplicitArg::Builtin(BuiltinName::bitwise),
    ];
    let (mut cairo_runner, program, entrypoint) = initialize_cairo_runner(
        &runner_config,
        OS_PROGRAM_BYTES,
        cairo_finalize_fn,
        &implicit_args,
        HashMap::new(),
    )
    .unwrap();

    let sha_start = cairo_runner
        .vm
        .gen_arg(
            &input.iter().map(|&word| MaybeRelocatable::Int(Felt::from(word))).collect::<Vec<_>>(),
        )
        .unwrap()
        .get_relocatable()
        .unwrap();
    let sha_end = (sha_start + input.len()).unwrap();

    let explicit_args = [
        EndpointArg::Value(ValueArg::Single(MaybeRelocatable::RelocatableValue(sha_start))),
        EndpointArg::Value(ValueArg::Single(MaybeRelocatable::RelocatableValue(sha_end))),
    ];
    run_cairo_0_entrypoint(
        entrypoint,
        &explicit_args,
        &implicit_args,
        None,
        &mut cairo_runner,
        &program,
        &runner_config,
        &[],
    )
    .unwrap();

    cairo_runner.get_execution_resources().unwrap()
}

/// Tests that the SHA-finalization Cairo functions from the OS program consume the expected
/// resources.
#[rstest]
#[case(
    "starkware.cairo.common.cairo_sha256.sha256_utils.finalize_sha256",
    8,
    SHA256_BATCH_SIZE,
    sha_256_update_state,
    &SHA256_BATCH_RESOURCES_LINEAR,
    &SHA256_BATCH_RESOURCES_CONSTANT,
)]
#[case(
    "starkware.cairo.common.cairo_sha512.sha512_utils.finalize_sha512",
    2,
    SHA512_BATCH_SIZE,
    sha_512_update_state,
    &SHA512_BATCH_RESOURCES_LINEAR,
    &SHA512_BATCH_RESOURCES_CONSTANT,
)]
fn test_finalize_sha<T>(
    #[case] cairo_finalize_fn: &str,
    #[case] number_of_blocks_1: usize,
    #[case] batch_size: usize,
    #[case] sha_update_state_fn: fn(&[T; 8], &[T; 16]) -> [T; 8],
    #[case] expected_linear_factor: &ExecutionResources,
    #[case] expected_constant_factor: &ExecutionResources,
) where
    T: Clone + Copy,
    StandardUniform: Distribution<T> + Distribution<[T; 8]> + Distribution<[T; 16]>,
    Felt: From<T>,
{
    // SHA batching resources has a factor that is linear in the number of rounds, and a constant
    // factor. Sample the execution at two points to compute both factors.
    let number_of_rounds_1 = (number_of_blocks_1 - 1) / batch_size + 1;
    let number_of_blocks_2 = number_of_blocks_1 + batch_size;
    let number_of_rounds_2 = (number_of_blocks_2 - 1) / batch_size + 1;
    let resources_1 = run_finalize_sha(number_of_blocks_1, cairo_finalize_fn, sha_update_state_fn);
    let resources_2 = run_finalize_sha(number_of_blocks_2, cairo_finalize_fn, sha_update_state_fn);

    assert_eq!(number_of_rounds_2 - number_of_rounds_1, 1);
    let linear_factor = (&resources_2 - &resources_1).filter_unused_builtins();
    let constant_factor =
        (&resources_1 - &(&linear_factor * number_of_rounds_1)).filter_unused_builtins();

    assert_eq!(&linear_factor, expected_linear_factor);
    assert_eq!(&constant_factor, expected_constant_factor);
}
