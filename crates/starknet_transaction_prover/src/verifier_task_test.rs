use std::io::Read;

use flate2::read::GzDecoder;
use starknet_os::proof_fact_fold::{compute_processed_proof_output_digest, pack_output_digest};
use starknet_types_core::felt::Felt;

use super::{run_circuit_verifier_task, verify_circuit_verifier_task_output, VerifierTaskError};

const SIMPLE_BOOTLOADER_PROGRAM_GZ: &[u8] =
    include_bytes!("../resources/simple_bootloader_compiled.json.gz");

const VERIFIER_EXECUTABLE_GZ: &[u8] =
    include_bytes!("../resources/stwo_circuit_verifier_canonical_small.executable.json.gz");

const PROCESSED_PROOF_GZ: &[u8] = include_bytes!("../resources/one_leaf_root_proof.json.gz");

const FIXTURE_VERIFIER_PROGRAM_HASH: Felt =
    Felt::from_hex_unchecked("0x764dc214c7f45a6899b05d42ba4d23d5849e0bf0383ec951f000b1742107fdd");

fn gunzip(compressed_bytes: &[u8]) -> Vec<u8> {
    let mut decompressed_bytes = Vec::new();
    GzDecoder::new(compressed_bytes).read_to_end(&mut decompressed_bytes).unwrap();
    decompressed_bytes
}

fn processed_proof_felts() -> Vec<Felt> {
    let processed_proof_hex: Vec<String> =
        serde_json::from_slice(&gunzip(PROCESSED_PROOF_GZ)).unwrap();
    processed_proof_hex
        .iter()
        .map(|proof_felt_hex| Felt::from_hex(proof_felt_hex).unwrap())
        .collect()
}

fn one_leaf_proof_facts() -> Vec<Felt> {
    [
        Felt::ZERO,
        Felt::ZERO,
        Felt::from_hex("0x32b88272d54b83880ebebd9c4292a650bee27d1575e82123391b6df2932e843")
            .unwrap(),
        Felt::from_hex("0xb").unwrap(),
        Felt::from_hex("0xd").unwrap(),
        Felt::from_hex("0x11").unwrap(),
    ]
    .to_vec()
}

#[test]
fn test_run_and_verify_circuit_verifier_task() {
    let simple_bootloader_program = gunzip(SIMPLE_BOOTLOADER_PROGRAM_GZ);
    let verifier_executable = gunzip(VERIFIER_EXECUTABLE_GZ);
    let task_output = run_circuit_verifier_task(
        &simple_bootloader_program,
        &verifier_executable,
        &processed_proof_felts(),
    )
    .unwrap();
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
    let simple_bootloader_program = gunzip(SIMPLE_BOOTLOADER_PROGRAM_GZ);
    let verifier_executable = gunzip(VERIFIER_EXECUTABLE_GZ);
    let mut corrupted_processed_proof_felts = processed_proof_felts();
    corrupted_processed_proof_felts[100] += Felt::ONE;
    assert!(matches!(
        run_circuit_verifier_task(
            &simple_bootloader_program,
            &verifier_executable,
            &corrupted_processed_proof_felts,
        ),
        Err(VerifierTaskError::VerifierRun(_))
    ));
}
