use std::fs;
use std::io::Read;

use apollo_infra_utils::path::resolve_project_relative_path;
use bzip2::read::BzDecoder;
use cairo_vm::vm::runners::cairo_pie::CairoPie;
use starknet_types_core::felt::Felt;

use crate::proving::prove;

/// Resolves a path relative to the crate's resources directory.
fn resolve_test_resource_path(relative_path: &str) -> std::io::Result<std::path::PathBuf> {
    let path = ["crates", "starknet_os_runner", "resources", relative_path]
        .iter()
        .collect::<std::path::PathBuf>();
    resolve_project_relative_path(&path.to_string_lossy())
}

/// Integration test that verifies proving works with a real Cairo PIE.
///
/// This test is ignored by default because it requires the `stwo_run_and_prove` binary.
/// Run with: `cargo test -p starknet_os_runner -- --ignored test_prove_cairo_pie_10_transfers`
#[tokio::test]
#[ignore]
async fn test_prove_cairo_pie_10_transfers() {
    let cairo_pie_path = resolve_test_resource_path("cairo_pie_10_transfers.zip")
        .expect("Failed to resolve cairo_pie_10_transfers.zip path");
    let expected_proof_path = resolve_test_resource_path("proof_1_success.bin")
        .expect("Failed to resolve proof_1_success.bin path");
    let expected_proof_facts_path = resolve_test_resource_path("proof_1_output.json")
        .expect("Failed to resolve proof_1_output.json path");

    // Read CairoPie from zip file.
    let cairo_pie =
        CairoPie::read_zip_file(&cairo_pie_path).expect("Failed to read Cairo PIE from zip file");

    // Prove the Cairo PIE.
    let output = prove(cairo_pie).await.expect("Failed to prove Cairo PIE");

    // Read expected proof (it's bzip2 compressed).
    let expected_proof_file =
        fs::File::open(&expected_proof_path).expect("Failed to open expected proof file");
    let mut expected_proof_bytes = Vec::new();
    let mut expected_bz_decoder = BzDecoder::new(expected_proof_file);
    expected_bz_decoder
        .read_to_end(&mut expected_proof_bytes)
        .expect("Failed to read expected proof file");

    // Encode expected proof bytes to u32s for comparison.
    let expected_proof = proving_utils::proof_encoding::encode_bytes_to_u32(&expected_proof_bytes);

    // Read expected proof facts.
    let expected_proof_facts_str = fs::read_to_string(&expected_proof_facts_path)
        .expect("Failed to read expected proof facts file");
    let expected_proof_facts: Vec<Felt> = serde_json::from_str(&expected_proof_facts_str)
        .expect("Failed to parse expected proof facts");

    // Compare proofs.
    assert_eq!(output.proof, expected_proof, "Generated proof does not match expected proof");

    // Compare proof facts.
    assert_eq!(
        output.proof_facts, expected_proof_facts,
        "Generated proof facts do not match expected proof facts"
    );
}
