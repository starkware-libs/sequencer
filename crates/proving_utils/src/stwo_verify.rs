//! Wrapper module for executing the `stwo_verify` external binary.
//!
//! This module provides a robust interface for invoking the `stwo_verify` tool,
//! which verifies a Stwo Cairo proof.
//!
//! # Binary Resolution
//!
//! The binary is resolved in the following order:
//! 1. Explicit `binary_path` in [`StwoVerifyConfig`] (if provided)
//! 2. `STWO_VERIFY_PATH` environment variable
//! 3. Local install location: `<repo_root>/target/tools/stwo_verify`
//! 4. PATH lookup (preferred in Docker containers where it's at `/usr/local/bin/`)
//!
//! # Installation
//!
//! For local development, run:
//! ```bash
//! scripts/install_stwo_verify.sh
//! ```
//!
//! After running the install script, the binary will be automatically found - no PATH
//! modification needed.
//!
//! In Docker/k8s, the binary is pre-installed at `/usr/local/bin/stwo_verify`.

use std::path::PathBuf;
use std::time::Duration;

use tokio::process::Command;

pub use crate::binary_runner::{BinaryOutput, ProofFormat};
use crate::binary_runner::{
    BinaryRunnerError, BinarySpec, DEFAULT_TIMEOUT, check_process_output, execute_with_timeout,
};

/// Environment variable for overriding the `stwo_verify` binary path.
pub const STWO_VERIFY_PATH_ENV: &str = "STWO_VERIFY_PATH";

/// Default binary name for PATH lookup.
const DEFAULT_BINARY_NAME: &str = "stwo_verify";

/// Relative path from repo root to the installed binary.
const INSTALL_RELATIVE_PATH: &str = "target/tools/stwo_verify";

/// Install script path for error messages.
const INSTALL_SCRIPT: &str = "scripts/install_stwo_verify.sh";

/// Errors that can occur when executing `stwo_verify`.
pub type StwoVerifyError = BinaryRunnerError;

/// Configuration for running `stwo_verify`.
#[derive(Debug, Clone)]
pub struct StwoVerifyConfig {
    /// Optional explicit path to the binary. If None, uses PATH lookup.
    pub binary_path: Option<PathBuf>,
    /// Timeout for the verification operation.
    pub timeout: Duration,
    /// Proof format for deserialization.
    pub proof_format: ProofFormat,
    /// Hash variant for the Merkle channel.
    pub channel_hash: ChannelHash,
    /// Preprocessed trace variant.
    pub preprocessed_trace: PreprocessedTrace,
}

impl Default for StwoVerifyConfig {
    fn default() -> Self {
        Self {
            binary_path: None,
            timeout: DEFAULT_TIMEOUT,
            proof_format: ProofFormat::CairoSerde,
            channel_hash: ChannelHash::Blake2s,
            preprocessed_trace: PreprocessedTrace::Canonical,
        }
    }
}

impl BinarySpec for StwoVerifyConfig {
    fn binary_name(&self) -> &str {
        DEFAULT_BINARY_NAME
    }

    fn env_var(&self) -> &str {
        STWO_VERIFY_PATH_ENV
    }

    fn install_relative_path(&self) -> &str {
        INSTALL_RELATIVE_PATH
    }

    fn install_script(&self) -> &str {
        INSTALL_SCRIPT
    }

    fn configured_path(&self) -> Option<&PathBuf> {
        self.binary_path.as_ref()
    }

    fn timeout(&self) -> Duration {
        self.timeout
    }
}

/// Hash variant for the Merkle channel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChannelHash {
    /// Blake2s-based Merkle channel.
    #[default]
    Blake2s,
    /// Poseidon252-based Merkle channel.
    Poseidon252,
}

impl ChannelHash {
    fn as_str(&self) -> &'static str {
        match self {
            ChannelHash::Blake2s => "blake2s",
            ChannelHash::Poseidon252 => "poseidon252",
        }
    }
}

/// Preprocessed trace variant.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PreprocessedTrace {
    /// Canonical preprocessed trace.
    #[default]
    Canonical,
    /// Canonical trace without pedersen.
    CanonicalWithoutPedersen,
}

impl PreprocessedTrace {
    fn as_str(&self) -> &'static str {
        match self {
            PreprocessedTrace::Canonical => "canonical",
            PreprocessedTrace::CanonicalWithoutPedersen => "canonical_without_pedersen",
        }
    }
}

/// Input for a `stwo_verify` invocation.
#[derive(Debug, Clone)]
pub struct StwoVerifyInput {
    /// Absolute path to the proof file.
    pub proof_path: PathBuf,
    /// Optional absolute path where the program output will be saved.
    pub program_output_path: Option<PathBuf>,
    /// Optional absolute path where the program hash will be saved.
    pub program_hash_output_path: Option<PathBuf>,
}

/// Runs `stwo_verify` with the given input and configuration.
///
/// # Arguments
///
/// * `input` - The input paths for the verification operation.
/// * `config` - Configuration for the verification operation.
///
/// # Returns
///
/// The output from the verification operation, or an error if it failed.
///
/// # Example
///
/// ```ignore
/// let input = StwoVerifyInput {
///     proof_path: PathBuf::from("/path/to/proof.json"),
///     program_output_path: Some(PathBuf::from("/path/to/output.json")),
///     program_hash_output_path: None,
/// };
///
/// let config = StwoVerifyConfig::default();
/// let output = run_stwo_verify(&input, &config).await?;
/// ```
pub async fn run_stwo_verify(
    input: &StwoVerifyInput,
    config: &StwoVerifyConfig,
) -> Result<BinaryOutput, StwoVerifyError> {
    // Resolve binary path.
    let binary_path = config.resolve_binary_path();

    // Build command.
    let mut command = Command::new(&binary_path);

    // Required arguments.
    command.arg("--proof_path").arg(&input.proof_path);

    // Configuration options.
    command.arg("--proof-format").arg(config.proof_format.as_str());
    command.arg("--channel_hash").arg(config.channel_hash.as_str());
    command.arg("--preprocessed_trace").arg(config.preprocessed_trace.as_str());

    // Optional arguments.
    if let Some(ref output_path) = input.program_output_path {
        command.arg("--program_output").arg(output_path);
    }

    if let Some(ref hash_path) = input.program_hash_output_path {
        command.arg("--program_hash_output").arg(hash_path);
    }

    // Set RUST_BACKTRACE for better error diagnostics.
    command.env("RUST_BACKTRACE", "1");

    // Log the command being executed (without sensitive data).
    tracing::info!(
        binary_path = %binary_path.display(),
        proof = %input.proof_path.display(),
        "Executing stwo_verify"
    );

    // Execute with timeout.
    let output = execute_with_timeout(&mut command, config).await?;

    // Check for errors.
    check_process_output(&output, config.binary_name())?;

    tracing::info!("stwo_verify completed successfully");

    Ok(BinaryOutput::from_output(&output))
}
