use std::any::Any;
use std::collections::HashMap;

use apollo_starknet_os_program::test_programs::{
    PROOF_FACT_FOLD_BYTES,
    VERIFY_PROCESSED_PROOF_TEST_BYTES,
};
use cairo_program_runner_lib::{BootloaderHintProcessor, SIMPLE_BOOTLOADER_INPUT};
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_types_core::felt::Felt;

use super::{
    combine_leaf_digests,
    compute_leaf_output_digest,
    compute_verification_digest,
    pack_output_digest,
    Blake2sDigestWords,
};
use crate::test_utils::cairo_runner::{
    initialize_and_run_cairo_0_entry_point,
    initialize_cairo_runner,
    run_cairo_0_entrypoint_with_hint_processor,
    Cairo0EntryPointRunnerResult,
    EndpointArg,
    EntryPointRunnerConfig,
    ImplicitArg,
    PointerArg,
};
use crate::test_utils::golden_leaf::{
    GoldenCircuitVerifierTask,
    GOLDEN_LEAF_OUTPUT_DIGEST,
    GOLDEN_PROCESSED_PROOF_OUTPUT_DIGEST,
    GOLDEN_PROOF_FACTS,
    GOLDEN_VERIFICATION_DIGEST,
};

fn entrypoint_runner_config() -> EntryPointRunnerConfig {
    EntryPointRunnerConfig {
        layout: LayoutName::all_cairo,
        verify_secure: false,
        ..Default::default()
    }
}

fn felt_array_arg(felts: &[Felt]) -> EndpointArg {
    EndpointArg::Pointer(PointerArg::Array(
        felts.iter().map(|felt| MaybeRelocatable::Int(*felt)).collect(),
    ))
}

fn assert_cairo_leaf_output_digest(proof_facts: &[Felt], expected_digest: &Blake2sDigestWords) {
    assert_cairo_function_returns_digest(
        "compute_leaf_output_digest",
        &[EndpointArg::from(Felt::from(proof_facts.len())), felt_array_arg(proof_facts)],
        expected_digest,
    );
}

fn assert_cairo_combine_leaf_digests(
    left_leaf_digest: &Blake2sDigestWords,
    right_leaf_digest: &Blake2sDigestWords,
    expected_digest: &Blake2sDigestWords,
) {
    assert_cairo_function_returns_digest(
        "combine_leaf_digests",
        &[
            felt_array_arg(&left_leaf_digest.map(Felt::from)),
            felt_array_arg(&right_leaf_digest.map(Felt::from)),
        ],
        expected_digest,
    );
}

fn assert_cairo_verification_digest(
    processed_proof_output_digest: &Blake2sDigestWords,
    expected_digest: &Blake2sDigestWords,
) {
    assert_cairo_function_returns_digest(
        "compute_verification_digest",
        &[felt_array_arg(&processed_proof_output_digest.map(Felt::from))],
        expected_digest,
    );
}

fn assert_cairo_function_returns_digest(
    function_name: &str,
    explicit_args: &[EndpointArg],
    expected_digest: &Blake2sDigestWords,
) {
    let expected_return_values = [felt_array_arg(&expected_digest.map(Felt::from))];
    let (_, explicit_return_values, _) = initialize_and_run_cairo_0_entry_point(
        &entrypoint_runner_config(),
        PROOF_FACT_FOLD_BYTES,
        function_name,
        explicit_args,
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        &expected_return_values,
        HashMap::new(),
        None,
    )
    .unwrap();
    assert_eq!(explicit_return_values, expected_return_values);
}

/// Runs the Cairo `verify_processed_proof` on the golden leaf's processed proof, against the packed
/// `processed_proof_output_digest`.
fn run_cairo_verify_processed_proof(
    processed_proof_output_digest: &Blake2sDigestWords,
) -> Cairo0EntryPointRunnerResult<()> {
    let verifier_task = GoldenCircuitVerifierTask::decompress();
    let simple_bootloader_input: Box<dyn Any> =
        Box::new(verifier_task.task_input().simple_bootloader_input().unwrap());
    let implicit_args = [
        BuiltinName::output,
        BuiltinName::pedersen,
        BuiltinName::range_check,
        BuiltinName::ec_op,
        BuiltinName::poseidon,
    ]
    .map(ImplicitArg::Builtin);
    let (mut cairo_runner, program, entrypoint) = initialize_cairo_runner(
        &entrypoint_runner_config(),
        VERIFY_PROCESSED_PROOF_TEST_BYTES,
        "verify_processed_proof_test",
        &implicit_args,
        HashMap::from([(SIMPLE_BOOTLOADER_INPUT.to_string(), simple_bootloader_input)]),
    )?;
    let (packed_low, packed_high) = pack_output_digest(processed_proof_output_digest);
    run_cairo_0_entrypoint_with_hint_processor(
        entrypoint,
        &[packed_low.into(), packed_high.into()],
        &implicit_args,
        &mut cairo_runner,
        &program,
        &entrypoint_runner_config(),
        &[],
        &mut BootloaderHintProcessor::new(None),
    )?;
    Ok(())
}

#[test]
fn test_leaf_output_digest_matches_proving_side_golden() {
    assert_eq!(compute_leaf_output_digest(&GOLDEN_PROOF_FACTS), GOLDEN_LEAF_OUTPUT_DIGEST);
    assert_cairo_leaf_output_digest(&GOLDEN_PROOF_FACTS, &GOLDEN_LEAF_OUTPUT_DIGEST);
}

#[test]
fn test_processed_proof_digests_match_proving_side_goldens() {
    assert_eq!(
        combine_leaf_digests(&GOLDEN_LEAF_OUTPUT_DIGEST, &GOLDEN_LEAF_OUTPUT_DIGEST),
        GOLDEN_PROCESSED_PROOF_OUTPUT_DIGEST
    );
    assert_cairo_combine_leaf_digests(
        &GOLDEN_LEAF_OUTPUT_DIGEST,
        &GOLDEN_LEAF_OUTPUT_DIGEST,
        &GOLDEN_PROCESSED_PROOF_OUTPUT_DIGEST,
    );
    assert_eq!(
        compute_verification_digest(&GOLDEN_PROCESSED_PROOF_OUTPUT_DIGEST),
        GOLDEN_VERIFICATION_DIGEST
    );
    assert_cairo_verification_digest(
        &GOLDEN_PROCESSED_PROOF_OUTPUT_DIGEST,
        &GOLDEN_VERIFICATION_DIGEST,
    );
}

/// The golden fills both verifier slots with the same leaf, so it cannot catch the two leaves
/// being swapped or one being written twice.
#[test]
fn test_cairo_combine_leaf_digests_matches_rust_for_two_different_leaves() {
    let left_leaf_digest = GOLDEN_LEAF_OUTPUT_DIGEST;
    let right_leaf_digest = compute_leaf_output_digest(&[Felt::ZERO, Felt::ZERO, Felt::ONE]);
    assert_cairo_combine_leaf_digests(
        &left_leaf_digest,
        &right_leaf_digest,
        &combine_leaf_digests(&left_leaf_digest, &right_leaf_digest),
    );
}

#[test]
fn test_cairo_verify_processed_proof_accepts_its_output_digest() {
    run_cairo_verify_processed_proof(&GOLDEN_PROCESSED_PROOF_OUTPUT_DIGEST).unwrap();
}

/// The output digest of a processed proof of other proof facts than the golden leaf's.
#[test]
fn test_cairo_verify_processed_proof_rejects_another_output_digest() {
    let other_leaf_digest = compute_leaf_output_digest(&[Felt::ZERO, Felt::ZERO, Felt::ONE]);
    let error = run_cairo_verify_processed_proof(&combine_leaf_digests(
        &other_leaf_digest,
        &other_leaf_digest,
    ))
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("The circuit verifier's output does not match the processed proof."),
        "Unexpected error: {error}"
    );
}
