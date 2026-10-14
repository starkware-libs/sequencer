use std::any::Any;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use apollo_starknet_os_program::test_programs::{
    PROOF_FACT_FOLD_BYTES,
    VERIFY_PROCESSED_PROOF_TEST_BYTES,
};
use cairo_program_runner_lib::{BootloaderHintProcessor, SIMPLE_BOOTLOADER_INPUT};
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::layout_name::LayoutName;
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_types_core::felt::Felt;
use tempfile::NamedTempFile;

use super::{
    combine_leaf_digests,
    compute_leaf_output_digest,
    compute_verification_digest,
    Blake2sDigestWords,
};
use crate::io::os_input::CircuitVerifierTaskInput;
use crate::test_utils::cairo_runner::{
    get_all_builtins_ordered,
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
    EntryPointRunnerConfig { layout: LayoutName::all_cairo, ..Default::default() }
}

fn felt_array_arg(felts: &[Felt]) -> EndpointArg {
    EndpointArg::Pointer(PointerArg::Array(
        felts.iter().map(|felt| MaybeRelocatable::Int(*felt)).collect(),
    ))
}

/// Runs the Cairo `compute_processed_proof_output_digest` on two transactions' proof facts, giving
/// it a pointer to `expected_digest` to write the output digest to. Cairo memory is write-once, so
/// the run fails unless the function writes `expected_digest`.
fn assert_cairo_processed_proof_output_digest(
    left_proof_facts: &[Felt],
    right_proof_facts: &[Felt],
    expected_digest: &Blake2sDigestWords,
) {
    initialize_and_run_cairo_0_entry_point(
        &entrypoint_runner_config(),
        PROOF_FACT_FOLD_BYTES,
        "compute_processed_proof_output_digest",
        &[
            EndpointArg::from(left_proof_facts.len()),
            felt_array_arg(left_proof_facts),
            EndpointArg::from(right_proof_facts.len()),
            felt_array_arg(right_proof_facts),
            felt_array_arg(&expected_digest.map(Felt::from)),
        ],
        &[ImplicitArg::Builtin(BuiltinName::range_check)],
        &[],
        HashMap::new(),
        None,
    )
    .unwrap();
}

/// Runs the Cairo `verify_processed_proof` on the given circuit verifier task, against
/// `proof_facts`.
fn run_cairo_verify_processed_proof(
    verifier_task_input: &CircuitVerifierTaskInput,
    proof_facts: &[Felt],
) -> Cairo0EntryPointRunnerResult<()> {
    let simple_bootloader_input: Box<dyn Any> =
        Box::new(verifier_task_input.simple_bootloader_input().unwrap());
    let implicit_args: Vec<ImplicitArg> =
        get_all_builtins_ordered()?.into_iter().map(ImplicitArg::Builtin).collect();
    let runner_config = entrypoint_runner_config();
    let (mut cairo_runner, program, entrypoint) = initialize_cairo_runner(
        &runner_config,
        VERIFY_PROCESSED_PROOF_TEST_BYTES,
        "verify_processed_proof_test",
        &implicit_args,
        HashMap::from([(SIMPLE_BOOTLOADER_INPUT.to_string(), simple_bootloader_input)]),
    )?;
    run_cairo_0_entrypoint_with_hint_processor(
        entrypoint,
        &[EndpointArg::from(proof_facts.len()), felt_array_arg(proof_facts)],
        &implicit_args,
        &mut cairo_runner,
        &program,
        &runner_config,
        &[],
        &mut BootloaderHintProcessor::new(None),
    )?;
    Ok(())
}

/// Writes a copy of the circuit verifier executable at `executable_path` whose last bytecode word
/// differs, which makes it a different program.
fn write_other_verifier_executable(executable_path: &Path) -> NamedTempFile {
    let mut executable: serde_json::Value =
        serde_json::from_reader(File::open(executable_path).unwrap()).unwrap();
    let bytecode = executable["program"]["bytecode"].as_array_mut().unwrap();
    *bytecode.last_mut().unwrap() = serde_json::Value::String("0x1".to_string());
    let other_executable_file = NamedTempFile::new().unwrap();
    serde_json::to_writer(other_executable_file.as_file(), &executable).unwrap();
    other_executable_file
}

#[test]
fn test_leaf_output_digest_matches_proving_side_golden() {
    assert_eq!(compute_leaf_output_digest(&GOLDEN_PROOF_FACTS), GOLDEN_LEAF_OUTPUT_DIGEST);
}

#[test]
fn test_processed_proof_digests_match_proving_side_goldens() {
    assert_eq!(
        combine_leaf_digests(&GOLDEN_LEAF_OUTPUT_DIGEST, &GOLDEN_LEAF_OUTPUT_DIGEST),
        GOLDEN_PROCESSED_PROOF_OUTPUT_DIGEST
    );
    assert_eq!(
        compute_verification_digest(&GOLDEN_PROCESSED_PROOF_OUTPUT_DIGEST),
        GOLDEN_VERIFICATION_DIGEST
    );
}

/// The processed proof output digest is computed from the leaf output digests, so this also checks
/// them.
#[test]
fn test_cairo_processed_proof_output_digest_matches_proving_side_golden() {
    assert_cairo_processed_proof_output_digest(
        &GOLDEN_PROOF_FACTS,
        &GOLDEN_PROOF_FACTS,
        &GOLDEN_PROCESSED_PROOF_OUTPUT_DIGEST,
    );
}

#[test]
fn test_cairo_verify_processed_proof_accepts_its_proof_facts() {
    let verifier_task = GoldenCircuitVerifierTask::decompress();
    run_cairo_verify_processed_proof(&verifier_task.task_input(), &GOLDEN_PROOF_FACTS).unwrap();
}

#[test]
fn test_cairo_verify_processed_proof_rejects_other_proof_facts() {
    let verifier_task = GoldenCircuitVerifierTask::decompress();
    let mut proof_facts = GOLDEN_PROOF_FACTS;
    // Flip a single felt in the proof facts.
    proof_facts[2] += Felt::ONE;
    let error =
        run_cairo_verify_processed_proof(&verifier_task.task_input(), &proof_facts).unwrap_err();
    assert!(
        error.to_string().contains("The processed proof does not verify against its proof facts."),
        "Unexpected error: {error}"
    );
}

/// The circuit verifier's program hash is pinned: another program fails the run when the simple
/// bootloader writes its program hash, before running it.
#[test]
fn test_cairo_verify_processed_proof_rejects_other_verifier_program() {
    let verifier_task = GoldenCircuitVerifierTask::decompress();
    let golden_task_input = verifier_task.task_input();
    let other_executable_file = write_other_verifier_executable(&golden_task_input.executable_path);
    let other_task_input = CircuitVerifierTaskInput {
        executable_path: other_executable_file.path().to_path_buf(),
        ..golden_task_input
    };
    let error = run_cairo_verify_processed_proof(&other_task_input, &GOLDEN_PROOF_FACTS)
        .unwrap_err()
        .to_string();
    assert!(error.contains("assert [output_ptr + 1] = program_hash;"), "Unexpected error: {error}");
}
