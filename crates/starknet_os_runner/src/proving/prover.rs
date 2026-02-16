//! Cairo PIE proving using the Stwo prover.
//!
//! Provides functionality to generate zero-knowledge proofs from Cairo PIE files.

use std::path::PathBuf;

use apollo_transaction_converter::ProgramOutput;
use cairo_vm::vm::runners::cairo_pie::CairoPie;
use proving_utils::proof_encoding::ProofBytes;
use proving_utils::simple_bootloader_input::SimpleBootloaderInput;
use proving_utils::stwo_run_and_prove::{
    run_stwo_run_and_prove,
    ProofFormat,
    StwoRunAndProveConfig,
    StwoRunAndProveInput,
};
use starknet_api::transaction::fields::Proof;
use tempfile::NamedTempFile;

use crate::errors::ProvingError;
use crate::proving::bootloader::resolve_bootloader_path;

/// Output from the prover containing the compressed proof and associated program output.
#[derive(Debug, Clone)]
pub(crate) struct ProverOutput {
    /// The proof packed as u32s (4 bytes per u32, big-endian, with padding prefix).
    pub proof: Proof,
    /// Raw program output from the bootloader (first element is number of tasks).
    pub program_output: ProgramOutput,
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
pub(crate) async fn prove(cairo_pie: CairoPie) -> Result<ProverOutput, ProvingError> {
    // Create temporary files.
    let create_temp_file_and_path = || -> Result<(NamedTempFile, PathBuf), ProvingError> {
        let file = NamedTempFile::new().map_err(ProvingError::CreateTempFile)?;
        let path = file.path().to_path_buf();
        Ok((file, path))
    };

    let (_cairo_pie_file, cairo_pie_path) = create_temp_file_and_path()?;
    let (_program_input_file, program_input_path) = create_temp_file_and_path()?;
    let (_proof_file, proof_path) = create_temp_file_and_path()?;
    let (_program_output_file, program_output_path) = create_temp_file_and_path()?;

    // Write Cairo PIE to zip file.
    cairo_pie
        .write_zip_file(&cairo_pie_path, true /* merge_extra_segments */)
        .map_err(ProvingError::WriteCairoPie)?;

    // Write program input.
    let program_input = SimpleBootloaderInput::from_cairo_pie_path(&cairo_pie_path);
    let program_input_str =
        serde_json::to_string(&program_input).map_err(ProvingError::SerializeProgramInput)?;
    std::fs::write(&program_input_path, &program_input_str)
        .map_err(ProvingError::WriteProgramInput)?;

    // Resolve bootloader path.
    let bootloader_path = resolve_bootloader_path().await?;

    // Build prover input.
    let input = StwoRunAndProveInput {
        program_path: bootloader_path,
        program_input_path: Some(program_input_path),
        prover_params_path: None,
        proof_output_path: proof_path.clone(),
        program_output_path: Some(program_output_path.clone()),
    };

    // Configure the prover.
    let config = StwoRunAndProveConfig { proof_format: ProofFormat::Binary, ..Default::default() };

    // Run the prover.
    run_stwo_run_and_prove(&input, &config).await?;

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
