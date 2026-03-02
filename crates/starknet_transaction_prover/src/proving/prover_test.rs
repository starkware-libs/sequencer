use std::fs;
use std::path::PathBuf;

use apollo_infra_utils::path::resolve_project_relative_path;
use apollo_transaction_converter::proof_verification::stwo_verify;
use apollo_transaction_converter::transaction_converter::BOOTLOADER_PROGRAM_HASH;
use apollo_transaction_converter::ProgramOutput;
use cairo_vm::types::program::Program;
use cairo_vm::vm::runners::cairo_pie::CairoPie;
use proving_utils::proof_encoding::ProofBytes;
use starknet_api::transaction::fields::VIRTUAL_SNOS;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::Blake2Felt252;

use crate::proving::prover::{prove, resolve_resource_path, BOOTLOADER_FILE};

/// Test resource file names.
const CAIRO_PIE_FILE: &str = "cairo_pie_10_transfers.zip";
const EXPECTED_PROOF_FACTS_FILE: &str = "proof_facts_10_transfers.json";

/// Integration test that verifies proving works with a real Cairo PIE.
///
/// Run with:
/// ```shell
/// rustup run nightly-2025-07-14 cargo test -p starknet_transaction_prover --release --features \
///     stwo_proving test_prove_cairo_pie_10_transfers
/// ```
#[tokio::test]
async fn test_prove_cairo_pie_10_transfers() {
    let cairo_pie_path = resolve_resource_path(CAIRO_PIE_FILE).unwrap();
    let expected_program_output_path = resolve_resource_path(EXPECTED_PROOF_FACTS_FILE).unwrap();

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
    let expected_program_output_str = fs::read_to_string(&expected_program_output_path)
        .expect("Failed to read expected program output file");
    let expected_program_output: ProgramOutput = serde_json::from_str(&expected_program_output_str)
        .expect("Failed to parse expected program output");

    // Compare program output.
    assert_eq!(
        output.program_output, expected_program_output,
        "Generated program output does not match expected program output"
    );
}

/// Regenerates the example proof fixtures used by `apollo_transaction_converter` tests.
///
/// Run manually with:
/// ```bash
/// cargo test -p starknet_transaction_prover --features stwo_proving -- --ignored regenerate_proof_fixtures
/// ```
#[tokio::test]
#[ignore]
async fn regenerate_proof_fixtures() {
    let cairo_pie_path = resolve_resource_path(CAIRO_PIE_FILE).unwrap();
    let cairo_pie =
        CairoPie::read_zip_file(&cairo_pie_path).expect("Failed to read Cairo PIE from zip file");

    let output = prove(cairo_pie).await.expect("Failed to prove Cairo PIE");

    // Save proof as bz2-compressed file.
    let proof_bytes = ProofBytes::try_from(output.proof).expect("Failed to encode proof");
    let proof_path = resolve_transaction_converter_resource("example_proof.bz2");
    proof_bytes.to_file(&proof_path).expect("Failed to write proof file");
    println!("Wrote proof to {}", proof_path.display());

    // Save proof facts as JSON.
    let proof_facts = output
        .program_output
        .try_into_proof_facts(VIRTUAL_SNOS)
        .expect("Failed to convert program output to proof facts");
    let proof_facts_json =
        serde_json::to_string_pretty(&proof_facts).expect("Failed to serialize proof facts");
    let proof_facts_path = resolve_transaction_converter_resource("example_proof_facts.json");
    fs::write(&proof_facts_path, proof_facts_json).expect("Failed to write proof facts file");
    println!("Wrote proof facts to {}", proof_facts_path.display());
}

fn resolve_transaction_converter_resource(file_name: &str) -> PathBuf {
    let relative_path: PathBuf =
        ["crates", "apollo_transaction_converter", "resources", file_name].iter().collect();
    resolve_project_relative_path(&relative_path.to_string_lossy())
        .unwrap_or_else(|_| panic!("Failed to resolve path for {file_name}"))
}

#[test]
fn test_simple_bootloader_program_hash_matches_expected() {
    let bootloader_path = resolve_resource_path(BOOTLOADER_FILE).unwrap();
    let program_bytes = fs::read(&bootloader_path).expect("Failed to read bootloader file");
    let program =
        Program::from_bytes(&program_bytes, Some("main")).expect("Failed to load bootloader");
    let program_data: Vec<Felt> = program
        .iter_data()
        .map(|entry| entry.get_int_ref().copied().expect("Program data must contain felts."))
        .collect();
    let program_hash = Blake2Felt252::encode_felt252_data_and_calc_blake_hash(&program_data);
    assert_eq!(
        program_hash, BOOTLOADER_PROGRAM_HASH,
        "Bootloader program hash does not match expected"
    );
}
