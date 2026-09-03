use starknet_os::proof_fact_fold::{fold_block_proof_facts, pack_output_digest};
use starknet_types_core::felt::Felt;

use super::test_utils::{
    four_leaves_proof_facts,
    gunzip,
    root_proof_felts,
    run_four_leaves_verifier_task,
    FIXTURE_VERIFIER_PROGRAM_HASH,
    SIMPLE_BOOTLOADER_PROGRAM_GZ,
    VERIFIER_EXECUTABLE_GZ,
};
use super::{run_circuit_verifier_task, verify_circuit_verifier_task_output, VerifierTaskError};

/// The whole chain on real data: the simple bootloader runs the circuit verifier on the
/// `four_leaves` root proof, and the verifier's output digest matches the digest
/// expected from the OS-side fold of the same four transactions' proof facts.
#[test]
fn test_run_and_verify_circuit_verifier_task() {
    let task_output = run_four_leaves_verifier_task().unwrap();
    assert_eq!(task_output.verifier_program_hash, FIXTURE_VERIFIER_PROGRAM_HASH);

    // The OS side of the comparison: fold the four transactions' proof facts to the
    // packed root output digest the OS would emit, and check the verifier against it.
    let proof_facts = four_leaves_proof_facts();
    let root_entry = fold_block_proof_facts(&[proof_facts.as_slice(); 4]);
    let (root_output_low, root_output_high) = pack_output_digest(&root_entry.output_digest);
    verify_circuit_verifier_task_output(
        &task_output,
        FIXTURE_VERIFIER_PROGRAM_HASH,
        root_output_low,
        root_output_high,
    )
    .unwrap();

    // A wrong pinned program hash and a wrong emitted digest are both rejected.
    assert!(matches!(
        verify_circuit_verifier_task_output(
            &task_output,
            Felt::ONE,
            root_output_low,
            root_output_high
        ),
        Err(VerifierTaskError::UnexpectedVerifierProgramHash { .. })
    ));
    assert!(matches!(
        verify_circuit_verifier_task_output(
            &task_output,
            FIXTURE_VERIFIER_PROGRAM_HASH,
            root_output_high,
            root_output_low
        ),
        Err(VerifierTaskError::FoldDigestMismatch { .. })
    ));
}

/// A corrupted proof felt makes the verifier panic, which the executable's entry wrapper
/// turns into an unsatisfiable assertion: the run fails and there is no output.
#[test]
fn test_corrupted_root_proof_fails_the_run() {
    let mut corrupted_root_proof_felts = root_proof_felts();
    corrupted_root_proof_felts[100] += Felt::ONE;
    assert!(matches!(
        run_circuit_verifier_task(
            &gunzip(SIMPLE_BOOTLOADER_PROGRAM_GZ),
            &gunzip(VERIFIER_EXECUTABLE_GZ),
            &corrupted_root_proof_felts,
        ),
        Err(VerifierTaskError::VerifierRun(_))
    ));
}
