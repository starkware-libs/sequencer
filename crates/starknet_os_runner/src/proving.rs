//! Cairo PIE proving using the Stwo prover.
//!
//! Provides functionality to generate zero-knowledge proofs from Cairo PIE files.

use std::io::Read;
use std::path::PathBuf;

use apollo_infra_utils::path::resolve_project_relative_path;
use bzip2::read::BzDecoder;
use cairo_vm::vm::runners::cairo_pie::CairoPie;
use proving_utils::proof_encoding::encode_bytes_to_u32;
use proving_utils::stwo_run_and_prove::{
    run_stwo_run_and_prove,
    ProofFormat,
    StwoRunAndProveConfig,
    StwoRunAndProveInput,
};
use starknet_types_core::felt::Felt;
use tempfile::NamedTempFile;
use tracing::{debug, info};

use crate::errors::ProvingError;

/// Returns the bootloader program path relative to the project root.
fn bootloader_program_path() -> PathBuf {
    ["crates", "starknet_os_runner", "resources", "simple_bootloader_compiled.json"]
        .iter()
        .collect()
}

/// Creates the program input JSON content for the bootloader.
fn create_program_input(cairo_pie_path: &str) -> String {
    serde_json::json!({
        "tasks": [{
            "type": "CairoPiePath",
            "path": cairo_pie_path,
            "program_hash_function": "blake"
        }],
        "fact_topologies_path": null,
        "single_page": true
    })
    .to_string()
}

/// Output from the prover containing the compressed proof and associated facts.
#[derive(Debug, Clone)]
pub struct ProverOutput {
    /// The proof packed as u32s (4 bytes per u32, big-endian, with length prefix).
    pub proof: Vec<u32>,
    pub proof_facts: Vec<Felt>,
}

/// Resolves the path to the bootloader program.
///
/// Constructs the path relative to the project root.
/// The file is expected to exist when the prover runs.
fn resolve_bootloader_path() -> PathBuf {
    resolve_project_relative_path(&bootloader_program_path().to_string_lossy())
        .expect("Failed to resolve bootloader path")
}

/// Proves a Cairo PIE using the stwo prover.
///
/// # Arguments
///
/// * `cairo_pie` - The Cairo PIE to prove.
///
/// # Returns
///
/// The prover output containing the proof and proof facts.
pub async fn prove(cairo_pie: CairoPie) -> Result<ProverOutput, ProvingError> {
    // Create temporary files.
    let cairo_pie_zip_file = NamedTempFile::new().map_err(ProvingError::CreateTempFile)?;
    let cairo_pie_zip_path = cairo_pie_zip_file.path().to_path_buf();
    let program_input_file = NamedTempFile::new().map_err(ProvingError::CreateTempFile)?;
    let program_input_path = program_input_file.path().to_path_buf();
    let proof_file = NamedTempFile::new().map_err(ProvingError::CreateTempFile)?;
    let proof_path = proof_file.path().to_path_buf();
    let proof_facts_file = NamedTempFile::new().map_err(ProvingError::CreateTempFile)?;
    let proof_facts_path = proof_facts_file.path().to_path_buf();

    // Write Cairo PIE to zip file.
    info!("Writing Cairo Pie to zip file.");
    cairo_pie
        .write_zip_file(&cairo_pie_zip_path, true /* merge_extra_segments */)
        .map_err(ProvingError::WriteCairoPie)?;
    info!(
        "Finished writing Cairo Pie to zip file. Zip file size: {} KB.",
        match std::fs::metadata(&cairo_pie_zip_path) {
            Err(_) => String::from("UNKNOWN"),
            Ok(meta) => format!("{}", meta.len() / 1024),
        }
    );

    // Write program input.
    let program_input = create_program_input(&cairo_pie_zip_path.to_string_lossy());
    info!("Writing program_input.json.");
    std::fs::write(&program_input_path, &program_input).map_err(ProvingError::WriteProgramInput)?;
    info!("Finished writing program_input.json.");
    debug!("Program input: {}", program_input);

    // Resolve bootloader path.
    let bootloader_path = resolve_bootloader_path();
    info!("Using bootloader at: {}", bootloader_path.display());

    // Build prover input.
    let input = StwoRunAndProveInput {
        program_path: bootloader_path,
        program_input_path: Some(program_input_path.clone()),
        prover_params_path: None,
        proof_output_path: proof_path.clone(),
        program_output_path: Some(proof_facts_path.clone()),
    };

    // Configure the prover.
    let config = StwoRunAndProveConfig { proof_format: ProofFormat::Binary, ..Default::default() };

    // Run the prover.
    info!("Proving Cairo Pie.");
    run_stwo_run_and_prove(&input, &config).await?;
    info!("Finished proving Cairo Pie.");

    info!("Proof file path: {}", proof_path.display());
    info!("Proof facts file path: {}", proof_facts_path.display());

    // Read and decompress the proof.
    info!("Reading proof from file.");
    let proof_file_handle = std::fs::File::open(&proof_path).map_err(ProvingError::ReadProof)?;
    let mut proof = Vec::new();
    let mut bz_decoder = BzDecoder::new(proof_file_handle);
    bz_decoder.read_to_end(&mut proof).map_err(ProvingError::ReadProof)?;
    info!("Finished reading proof from file. Proof size: {} KB.", proof.len() / 1024);

    // Read and parse proof facts.
    info!("Reading proof facts from file.");
    let proof_facts_str =
        std::fs::read_to_string(&proof_facts_path).map_err(ProvingError::ReadProofFacts)?;
    let proof_facts: Vec<Felt> =
        serde_json::from_str(&proof_facts_str).map_err(ProvingError::ParseProofFacts)?;
    info!("Finished reading proof facts from file. Number of proof facts: {}.", proof_facts.len());

    // Encode the proof bytes as u32s (4 bytes per u32, big-endian, with length prefix).
    let proof = encode_bytes_to_u32(&proof);

    Ok(ProverOutput { proof, proof_facts })
}
