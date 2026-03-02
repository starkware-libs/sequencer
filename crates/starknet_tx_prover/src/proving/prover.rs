//! Cairo PIE proving using the Stwo prover.
//!
//! Provides functionality to generate zero-knowledge proofs from Cairo PIE files.

use std::path::PathBuf;

use apollo_infra_utils::path::resolve_project_relative_path;
use apollo_transaction_converter::ProgramOutput;
use cairo_air::utils::ProofFormat;
use cairo_vm::vm::runners::cairo_pie::CairoPie;
use proving_utils::proof_encoding::ProofBytes;
use starknet_api::transaction::fields::Proof;
use stwo_run_and_prove_lib::ProveConfig;
use tempfile::NamedTempFile;

use crate::errors::ProvingError;
use crate::proving::error::StwoRunAndProveError;
use crate::proving::stwo_run_and_prove::prove_pie_in_memory;

/// Bootloader program file name.
pub(crate) const BOOTLOADER_FILE: &str = "simple_bootloader_compiled.json";

/// Prover parameters file name.
const PROVER_PARAMS_FILE: &str = "prover_params.json";

/// Output from the prover containing the compressed proof and associated program output.
#[derive(Debug, Clone)]
pub(crate) struct ProverOutput {
    /// The proof packed as u32s (4 bytes per u32, big-endian, with padding prefix).
    pub proof: Proof,
    /// Raw program output from the bootloader (first element is number of tasks).
    pub program_output: ProgramOutput,
}

/// Resolves a path to a resource file in the crate's resources directory.
/// Constructs the path relative to the project root.
pub(crate) fn resolve_resource_path(file_name: &str) -> Result<PathBuf, ProvingError> {
    let path = ["crates", "starknet_tx_prover", "resources", file_name].iter().collect::<PathBuf>();
    resolve_project_relative_path(&path.to_string_lossy()).map_err(|source| {
        ProvingError::ResolveResourcePath { file_name: file_name.to_string(), source }
    })
}

/// Proves a Cairo PIE using the stwo prover.
///
/// # Arguments
///
/// * `cairo_pie` - The Cairo PIE to prove.
///
/// # Returns
///
/// The prover output containing the proof and program output.
pub(crate) async fn prove(cairo_pie: CairoPie) -> Result<ProverOutput, ProvingError> {
    // Create temporary files for output only.
    let (_proof_file, proof_path) = create_temp_file_and_path()?;
    let (_program_output_file, program_output_path) = create_temp_file_and_path()?;

    // Resolve the prover params and bootloader program paths.
    let prover_params_path = resolve_resource_path(PROVER_PARAMS_FILE)?;
    let bootloader_path = resolve_resource_path(BOOTLOADER_FILE)?;

    // Configure the prover.
    let prove_config = ProveConfig {
        proof_path: proof_path.clone(),
        proof_format: ProofFormat::Binary,
        verify: false,
        prover_params_json: Some(prover_params_path),
    };

    // Run the prover with in-memory CairoPie on a blocking thread.
    let output_path = program_output_path.clone();
    tokio::task::spawn_blocking(move || {
        prove_pie_in_memory(bootloader_path, cairo_pie, Some(output_path), prove_config)
    })
    .await
    .map_err(StwoRunAndProveError::from)?
    .map_err(StwoRunAndProveError::from)?;

    // Read and decompress the proof.
    let proof_bytes = ProofBytes::from_file(&proof_path).map_err(ProvingError::ReadProof)?;

    // Read and parse program output.
    let program_output_str =
        std::fs::read_to_string(&program_output_path).map_err(ProvingError::ReadProofFacts)?;
    let program_output: ProgramOutput =
        serde_json::from_str(&program_output_str).map_err(ProvingError::ParseProofFacts)?;

    // Convert proof bytes to packed u32 format.
    let proof: Proof = proof_bytes.into();

    Ok(ProverOutput { proof, program_output })
}

fn create_temp_file_and_path() -> Result<(NamedTempFile, PathBuf), ProvingError> {
    let file = NamedTempFile::new().map_err(ProvingError::CreateTempFile)?;
    let path = file.path().to_path_buf();
    Ok((file, path))
}
