use std::fs;

use cairo_vm::vm::runners::cairo_pie::CairoPie;
use starknet_api::transaction::fields::ProofFacts;

use crate::proving::prover::{prove, resolve_resource_path};

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
    // TODO(Avi): Verify the proof.

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
