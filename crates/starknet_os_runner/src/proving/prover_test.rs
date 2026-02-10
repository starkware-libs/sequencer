use std::fs;

use apollo_transaction_converter::proof_verification::stwo_verify;
use apollo_transaction_converter::transaction_converter::BOOTLOADER_PROGRAM_HASH;
use apollo_transaction_converter::ProgramOutput;
use cairo_vm::types::program::Program;
use cairo_vm::vm::runners::cairo_pie::CairoPie;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::Blake2Felt252;

use crate::proving::prover::{prove, resolve_resource_path, BOOTLOADER_FILE};

/// Test resource file names.
const CAIRO_PIE_FILE: &str = "cairo_pie_10_transfers.zip";
const EXPECTED_PROOF_FACTS_FILE: &str = "proof_facts_10_transfers.json";

/// Integration test that verifies proving works with a real Cairo PIE.
///
/// This test is ignored by default because it requires the `stwo_run_and_prove` binary.
/// Run with: `cargo test -p starknet_os_runner -- --ignored test_prove_cairo_pie_10_transfers`
#[tokio::test]
#[ignore]
async fn test_prove_cairo_pie_10_transfers() {
    let cairo_pie_path = resolve_resource_path(CAIRO_PIE_FILE).unwrap();
    let expected_proof_facts_path = resolve_resource_path(EXPECTED_PROOF_FACTS_FILE).unwrap();

    // Read CairoPie from zip file.
    let cairo_pie =
        CairoPie::read_zip_file(&cairo_pie_path).expect("Failed to read Cairo PIE from zip file");

    // Prove the Cairo PIE.
    let output = prove(cairo_pie).await.expect("Failed to prove Cairo PIE");

    // Verify the proof.
    let verify_output = stwo_verify(output.proof.clone()).expect("Failed to verify proof");

    // Check that the verified program output matches the prover output.
    assert_eq!(
        verify_output.program_output, output.program_output,
        "Verified program output does not match prover output"
    );

    // Check that the program hash matches the expected bootloader hash.
    assert_eq!(
        verify_output.program_hash, BOOTLOADER_PROGRAM_HASH,
        "Program hash does not match expected bootloader hash"
    );

    // Read expected program output.
    let expected_program_output_str = fs::read_to_string(&expected_proof_facts_path)
        .expect("Failed to read expected program output file");
    let expected_program_output: ProgramOutput = serde_json::from_str(&expected_program_output_str)
        .expect("Failed to parse expected program output");

    // Compare program output.
    assert_eq!(
        output.program_output, expected_program_output,
        "Generated program output does not match expected program output"
    );
}

#[test]
fn test_simple_bootloader_program_hash_matches_expected() {
    let bootloader_path = resolve_resource_path(BOOTLOADER_FILE).unwrap();
    let program_bytes = fs::read(&bootloader_path).expect("Failed to read bootloader file");
    let program =
        Program::from_bytes(&program_bytes, Some("main")).expect("Failed to load bootloader");
    let stripped = program.get_stripped_program().expect("Failed to strip program");
    let program_data: Vec<Felt> = stripped
        .data
        .iter()
        .map(|entry| entry.get_int_ref().copied().expect("Program data must contain felts."))
        .collect();
    let program_hash = Blake2Felt252::encode_felt252_data_and_calc_blake_hash(&program_data);
    assert_eq!(
        program_hash, BOOTLOADER_PROGRAM_HASH,
        "Bootloader program hash does not match expected"
    );
}
