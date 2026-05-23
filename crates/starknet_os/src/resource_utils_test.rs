use std::collections::HashMap;

use apollo_starknet_os_program::OS_PROGRAM_BYTES;
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
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
    SHA256_BLOCK_TO_ROUND,
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

fn run_finalize_sha256(number_of_blocks: usize) -> ExecutionResources {
    // Build the SHA-256 instance array. Each instance is 32 felts:
    // [message (16) | initial_state (8) | output_state (8)].
    let mut rng = StdRng::seed_from_u64(42);
    let mut input: Vec<u32> = Vec::new();
    for _ in 0..number_of_blocks {
        let random_felts: Vec<u32> = (0..24).map(|_| rng.gen::<u32>()).collect();
        input.extend_from_slice(&random_felts);
        let len = input.len();
        let message: &[u32; 16] = input[len - 24..len - 8].try_into().unwrap();
        let state: &[u32; 8] = input[len - 8..].try_into().unwrap();
        let output_state = sha_256_update_state(state, message);
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
        "starkware.cairo.common.cairo_sha256.sha256_utils.finalize_sha256",
        &implicit_args,
        HashMap::new(),
    )
    .unwrap();

    let sha256_start = cairo_runner
        .vm
        .gen_arg(
            &input.iter().map(|&word| MaybeRelocatable::Int(Felt::from(word))).collect::<Vec<_>>(),
        )
        .unwrap()
        .get_relocatable()
        .unwrap();
    let sha256_end = (sha256_start + input.len()).unwrap();

    let explicit_args = [
        EndpointArg::Value(ValueArg::Single(MaybeRelocatable::RelocatableValue(sha256_start))),
        EndpointArg::Value(ValueArg::Single(MaybeRelocatable::RelocatableValue(sha256_end))),
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

/// Tests that the `finalize_sha256` Cairo function from the OS program consumes the expected
/// resources.
#[test]
fn test_finalize_sha256() {
    // Sha256 batching resources has a linear factor and a constant factor. Sample the execution at
    // two points to compute both factors.
    let number_of_blocks_1 = 8_usize;
    let number_of_rounds_1 = (number_of_blocks_1 - 1) / SHA256_BLOCK_TO_ROUND + 1;
    let number_of_blocks_2 = number_of_blocks_1 + SHA256_BLOCK_TO_ROUND;
    let number_of_rounds_2 = (number_of_blocks_2 - 1) / SHA256_BLOCK_TO_ROUND + 1;
    let resources_1 = run_finalize_sha256(number_of_blocks_1);
    let resources_2 = run_finalize_sha256(number_of_blocks_2);

    assert_eq!(number_of_rounds_2 - number_of_rounds_1, 1);
    let linear_factor = (&resources_2 - &resources_1).filter_unused_builtins();
    let constant_factor =
        (&resources_1 - &(&linear_factor * number_of_rounds_1)).filter_unused_builtins();

    assert_eq!(linear_factor, *SHA256_BATCH_RESOURCES_LINEAR);
    assert_eq!(constant_factor, *SHA256_BATCH_RESOURCES_CONSTANT);
}
