use starknet_os::proof_fact_fold::{compute_processed_proof_output_digest, pack_output_digest};
use starknet_types_core::felt::Felt;

use super::test_utils::{
    gunzip,
    one_leaf_proof_facts,
    processed_proof_felts,
    run_one_leaf_verifier_task,
    FIXTURE_VERIFIER_PROGRAM_HASH,
    SIMPLE_BOOTLOADER_PROGRAM_GZ,
    VERIFIER_EXECUTABLE_GZ,
};
use super::{run_circuit_verifier_task, verify_circuit_verifier_task_output, VerifierTaskError};

#[test]
fn test_run_and_verify_circuit_verifier_task() {
    let task_output = run_one_leaf_verifier_task().unwrap();
    assert_eq!(task_output.verifier_program_hash, FIXTURE_VERIFIER_PROGRAM_HASH);

    // The single transaction's proof fills both of the multiverifier's verifier slots.
    let one_leaf_proof_facts = one_leaf_proof_facts();
    let output_digest =
        compute_processed_proof_output_digest(&one_leaf_proof_facts, &one_leaf_proof_facts);
    let (packed_output_low, packed_output_high) = pack_output_digest(&output_digest);
    verify_circuit_verifier_task_output(
        &task_output,
        FIXTURE_VERIFIER_PROGRAM_HASH,
        packed_output_low,
        packed_output_high,
    )
    .unwrap();

    assert!(matches!(
        verify_circuit_verifier_task_output(
            &task_output,
            Felt::ONE,
            packed_output_low,
            packed_output_high
        ),
        Err(VerifierTaskError::UnexpectedVerifierProgramHash { .. })
    ));
    assert!(matches!(
        verify_circuit_verifier_task_output(
            &task_output,
            FIXTURE_VERIFIER_PROGRAM_HASH,
            packed_output_high,
            packed_output_low
        ),
        Err(VerifierTaskError::VerificationDigestMismatch { .. })
    ));
}

#[test]
fn test_corrupted_processed_proof_fails_the_run() {
    let mut corrupted_processed_proof_felts = processed_proof_felts();
    corrupted_processed_proof_felts[100] += Felt::ONE;
    assert!(matches!(
        run_circuit_verifier_task(
            &gunzip(SIMPLE_BOOTLOADER_PROGRAM_GZ),
            &gunzip(VERIFIER_EXECUTABLE_GZ),
            &corrupted_processed_proof_felts,
        ),
        Err(VerifierTaskError::VerifierRun(_))
    ));
}
