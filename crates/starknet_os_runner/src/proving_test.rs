use std::fs;
use std::io::Read;

use bzip2::read::BzDecoder;
use cairo_vm::vm::runners::cairo_pie::CairoPie;
use proving_utils::proof_encoding::ProofBytes;
use starknet_api::transaction::fields::{Proof, ProofFacts};
use starknet_types_core::felt::Felt;

use crate::proving::{prove, resolve_resource_path};

/// Test resource file names.
const CAIRO_PIE_FILE: &str = "cairo_pie_10_transfers.zip";
const EXPECTED_PROOF_FILE: &str = "proof_1_success.bz2";
const EXPECTED_PROOF_FACTS_FILE: &str = "proof_1_output.json";

/// Integration test that verifies proving works with a real Cairo PIE.
///
/// This test is ignored by default because it requires the `stwo_run_and_prove` binary.
/// Run with: `cargo test -p starknet_os_runner -- --ignored test_prove_cairo_pie_10_transfers`
#[tokio::test]
#[ignore]
async fn test_prove_cairo_pie_10_transfers() {
    let cairo_pie_path = resolve_resource_path(CAIRO_PIE_FILE).unwrap();
    let expected_proof_path = resolve_resource_path(EXPECTED_PROOF_FILE).unwrap();
    let expected_proof_facts_path = resolve_resource_path(EXPECTED_PROOF_FACTS_FILE).unwrap();

    // Read CairoPie from zip file.
    let cairo_pie =
        CairoPie::read_zip_file(&cairo_pie_path).expect("Failed to read Cairo PIE from zip file");

    // Prove the Cairo PIE.
    let output = prove(cairo_pie).await.expect("Failed to prove Cairo PIE");

    // Read expected proof.
    let expected_proof: Proof = ProofBytes::from_file(&expected_proof_path)
        .expect("Failed to decode expected proof bytes")
        .into();

    // Read expected proof facts.
    let expected_proof_facts_str = fs::read_to_string(&expected_proof_facts_path)
        .expect("Failed to read expected proof facts file");
    let expected_proof_facts: ProofFacts = serde_json::from_str(&expected_proof_facts_str)
        .expect("Failed to parse expected proof facts");

    // Compare proofs.
    assert_eq!(output.proof, expected_proof, "Generated proof does not match expected proof");

    // Compare proof facts.
    assert_eq!(
        output.proof_facts, expected_proof_facts,
        "Generated proof facts do not match expected proof facts"
    );
}
