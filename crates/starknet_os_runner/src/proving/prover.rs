//! Cairo PIE proving using the Stwo prover.
//!
//! Provides functionality to generate zero-knowledge proofs from Cairo PIE files.

use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use apollo_infra_utils::path::project_path;
#[cfg(test)]
use apollo_infra_utils::path::resolve_project_relative_path;
use apollo_transaction_converter::ProgramOutput;
use cairo_vm::vm::runners::cairo_pie::CairoPie;
use proving_utils::proof_encoding::ProofBytes;
use proving_utils::simple_bootloader_input::SimpleBootloaderInput;
use proving_utils::stwo_run_and_prove::{
    ProofFormat, StwoRunAndProveConfig, StwoRunAndProveInput, run_stwo_run_and_prove,
};
use sha2::{Digest, Sha256};
use starknet_api::transaction::fields::Proof;
use tempfile::NamedTempFile;
use tracing::info;

use crate::errors::ProvingError;

/// Bootloader program file name.
pub(crate) const BOOTLOADER_FILE: &str = "simple_bootloader_compiled.json";
/// SHA-256 of the full bootloader JSON file from proving-utils.
pub(crate) const BOOTLOADER_JSON_SHA256: &str =
    "f6d235eb6a7f97038105ed9b6e0e083b11def61c664a17fe157135f9615efc76";
/// Pinned proving-utils revision that contains the bootloader JSON.
const PROVING_UTILS_REV: &str = "e16f9d0";
/// URL of the pinned bootloader JSON in proving-utils.
const BOOTLOADER_DOWNLOAD_URL: &str = "https://raw.githubusercontent.com/starkware-libs/proving-utils/e16f9d0/crates/cairo-program-runner-lib/resources/compiled_programs/bootloaders/simple_bootloader_compiled.json";

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
#[cfg(test)]
pub(crate) fn resolve_resource_path(file_name: &str) -> Result<PathBuf, ProvingError> {
    let path = ["crates", "starknet_os_runner", "resources", file_name].iter().collect::<PathBuf>();
    resolve_project_relative_path(&path.to_string_lossy()).map_err(|source| {
        ProvingError::ResolveResourcePath { file_name: file_name.to_string(), source }
    })
}

/// Resolves a local path for the bootloader JSON, downloading and verifying it if needed.
pub(crate) async fn resolve_bootloader_path() -> Result<PathBuf, ProvingError> {
    let bootloader_path = bootloader_cache_path()?;

    match std::fs::read(&bootloader_path) {
        Ok(cached_bootloader_bytes) => {
            if is_expected_bootloader_hash(&cached_bootloader_bytes) {
                return Ok(bootloader_path);
            }
        }
        Err(source) if source.kind() == ErrorKind::NotFound => {}
        Err(source) => {
            return Err(ProvingError::ReadBootloaderCache {
                path: bootloader_path.display().to_string(),
                source,
            });
        }
    }

    info!(
        bootloader_url = BOOTLOADER_DOWNLOAD_URL,
        proving_utils_rev = PROVING_UTILS_REV,
        cache_path = %bootloader_path.display(),
        "Downloading bootloader JSON."
    );
    download_bootloader_to_path(&bootloader_path).await?;
    Ok(bootloader_path)
}

fn bootloader_cache_path() -> Result<PathBuf, ProvingError> {
    let project_root = project_path().map_err(ProvingError::ResolveProjectRootPath)?;
    Ok(project_root.join("target").join("starknet_os_runner").join(BOOTLOADER_FILE))
}

async fn download_bootloader_to_path(bootloader_path: &Path) -> Result<(), ProvingError> {
    let response =
        reqwest::get(BOOTLOADER_DOWNLOAD_URL).await.map_err(ProvingError::DownloadBootloader)?;
    let bootloader_bytes = response
        .error_for_status()
        .map_err(ProvingError::DownloadBootloader)?
        .bytes()
        .await
        .map_err(ProvingError::DownloadBootloader)?;

    verify_bootloader_hash(&bootloader_bytes)?;
    write_bootloader_cache(bootloader_path, &bootloader_bytes)
}

fn write_bootloader_cache(
    bootloader_path: &Path,
    bootloader_bytes: &[u8],
) -> Result<(), ProvingError> {
    let parent_dir = bootloader_path.parent().ok_or_else(|| {
        ProvingError::InvalidBootloaderPath { path: bootloader_path.display().to_string() }
    })?;
    std::fs::create_dir_all(parent_dir).map_err(|source| {
        ProvingError::CreateBootloaderCacheDir { path: parent_dir.display().to_string(), source }
    })?;

    let mut temp_file = NamedTempFile::new_in(parent_dir).map_err(ProvingError::CreateTempFile)?;
    temp_file.write_all(bootloader_bytes).map_err(|source| ProvingError::WriteBootloaderCache {
        path: bootloader_path.display().to_string(),
        source,
    })?;
    temp_file.persist(bootloader_path).map_err(|source| ProvingError::PersistBootloaderCache {
        path: bootloader_path.display().to_string(),
        source: source.error,
    })?;

    Ok(())
}

fn verify_bootloader_hash(bootloader_bytes: &[u8]) -> Result<(), ProvingError> {
    let actual = calculate_sha256_hex(bootloader_bytes);
    if actual != BOOTLOADER_JSON_SHA256 {
        return Err(ProvingError::BootloaderHashMismatch {
            expected: BOOTLOADER_JSON_SHA256.to_string(),
            actual,
        });
    }
    Ok(())
}

fn is_expected_bootloader_hash(bootloader_bytes: &[u8]) -> bool {
    calculate_sha256_hex(bootloader_bytes) == BOOTLOADER_JSON_SHA256
}

fn calculate_sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
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
