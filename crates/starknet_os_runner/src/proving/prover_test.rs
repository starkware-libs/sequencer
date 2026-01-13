use std::fs;

use apollo_transaction_converter::proof_verification::verify_proof;
use apollo_transaction_converter::transaction_converter::BOOTLOADER_PROGRAM_HASH;
use cairo_vm::types::program::Program;
use cairo_vm::vm::runners::cairo_pie::CairoPie;
use starknet_api::transaction::fields::ProofFacts;
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
    let verify_output = verify_proof(output.proof.clone()).expect("Failed to verify proof");

    // Check that the verified proof facts match the prover output.
    assert_eq!(
        verify_output.proof_facts, output.proof_facts,
        "Verified proof facts do not match prover output"
    );

    // Check that the program hash matches the expected bootloader hash.
    let expected_program_hash =
        Felt::from_hex(BOOTLOADER_PROGRAM_HASH).expect("Invalid bootloader hash");
    assert_eq!(
        verify_output.program_hash, expected_program_hash,
        "Program hash does not match expected bootloader hash"
    );

    // Read expected proof facts.
    let expected_proof_facts_str = fs::read_to_string(&expected_proof_facts_path)
        .expect("Failed to read expected proof facts file");
    let expected_proof_facts: ProofFacts = serde_json::from_str(&expected_proof_facts_str)
        .expect("Failed to parse expected proof facts");

    // Compare proof facts.
    assert_eq!(
        output.proof_facts, expected_proof_facts,
        "Generated proof facts do not match expected proof facts"
    );
}

#[test]
fn test_simple_bootloader_program_hash_matches_expected() {
    let bootloader_path = resolve_resource_path(BOOTLOADER_FILE).unwrap();
    let program_bytes = fs::read(&bootloader_path).expect("Failed to read bootloader file");
    let program =
        Program::from_bytes(&program_bytes, Some("main")).expect("Failed to load bootloader");
    let stripped_program =
        program.get_stripped_program().expect("Failed to strip bootloader program");
    let program_data: Vec<Felt> = stripped_program
        .data
        .iter()
        .map(|entry| {
            entry.get_int_ref().copied().expect("Bootloader program data must contain felts")
        })
        .collect();
    let program_hash = Blake2Felt252::encode_felt252_data_and_calc_blake_hash(&program_data);
    let expected_hash =
        Felt::from_hex(BOOTLOADER_PROGRAM_HASH.trim()).expect("Invalid bootloader hash");

    assert_eq!(program_hash, expected_hash, "Bootloader program hash does not match expected");
}
