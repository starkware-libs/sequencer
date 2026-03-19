//! Binary for generating proof flow fixture files (`proof.bin` and `proof_facts.json`).
//!
//! Reads a CairoPie from a zip file, proves it using the stwo prover, and writes the
//! resulting proof and proof facts to
//! `crates/apollo_integration_tests/resources/proof_flow/`.
//!
//! Requires the `stwo_proving` feature and a nightly Rust toolchain.
//!
//! # Usage
//!
//! ```bash
//! CAIRO_PIE_PATH=/tmp/proof_flow_cairo_pie.zip \
//! cargo +nightly-2025-07-14 run --features stwo_proving \
//!     --bin generate_proof_flow_fixtures
//! ```
//!
//! # Environment variables
//!
//! - `CAIRO_PIE_PATH` — path to the CairoPie zip file
//!   (default: `/tmp/proof_flow_cairo_pie.zip`).

use cairo_vm::vm::runners::cairo_pie::CairoPie;
use starknet_transaction_prover::prove_cairo_pie_standalone;

fn main() {
    let cairo_pie_path = std::env::var("CAIRO_PIE_PATH")
        .unwrap_or_else(|_| "/tmp/proof_flow_cairo_pie.zip".to_string());

    println!("Reading CairoPie from {cairo_pie_path}");
    let cairo_pie = CairoPie::read_zip_file(std::path::Path::new(&cairo_pie_path))
        .expect("Failed to read CairoPie from zip file");

    println!("Proving CairoPie (this may take 5–10 minutes)...");
    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let (proof, proof_facts) =
        rt.block_on(prove_cairo_pie_standalone(cairo_pie)).expect("Proving failed");

    let resources_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CARGO_MANIFEST_DIR has no parent")
        .join("apollo_integration_tests/resources/proof_flow");
    std::fs::create_dir_all(&resources_dir).expect("Failed to create resources dir");

    let proof_facts_json =
        serde_json::to_string_pretty(&proof_facts).expect("Failed to serialize proof_facts");
    std::fs::write(resources_dir.join("proof_facts.json"), proof_facts_json)
        .expect("Failed to write proof_facts.json");

    std::fs::write(resources_dir.join("proof.bin"), proof.0.as_ref())
        .expect("Failed to write proof.bin");

    println!("Fixtures written to {}", resources_dir.display());
}
