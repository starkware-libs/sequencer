//! Wrapper module for executing the `stwo_run_and_prove` external binary.
//!
//! This module provides a robust interface for invoking the `stwo_run_and_prove` tool,
//! which runs a Cairo program and generates a Stwo proof for it.
//!
//! # Binary Resolution
//!
//! The binary is resolved in the following order:
//! 1. Explicit `binary_path` in [`StwoRunAndProveConfig`] (if provided)
//! 2. `STWO_RUN_AND_PROVE_PATH` environment variable
//! 3. Local install location: `<repo_root>/target/tools/stwo_run_and_prove`
//! 4. PATH lookup (preferred in Docker containers where it's at `/usr/local/bin/`)
//!
//! # Installation
//!
//! For local development, run:
//! ```bash
//! scripts/install_stwo_run_and_prove.sh
//! ```
//!
//! After running the install script, the binary will be automatically found - no PATH
//! modification needed.
//!
//! In Docker/k8s, the binary is pre-installed at `/usr/local/bin/stwo_run_and_prove`.

use std::path::PathBuf;
use std::time::Duration;

use tokio::process::Command;

pub use crate::binary_runner::{BinaryOutput, ProofFormat};
use crate::binary_runner::{
    BinaryRunnerError, BinarySpec, DEFAULT_TIMEOUT, check_process_output, execute_with_timeout,
};

/// Environment variable for overriding the `stwo_run_and_prove` binary path.
pub const STWO_RUN_AND_PROVE_PATH_ENV: &str = "STWO_RUN_AND_PROVE_PATH";

/// Default binary name for PATH lookup.
const DEFAULT_BINARY_NAME: &str = "stwo_run_and_prove";

/// Relative path from repo root to the installed binary.
const INSTALL_RELATIVE_PATH: &str = "target/tools/stwo_run_and_prove";

/// Install script path for error messages.
const INSTALL_SCRIPT: &str = "scripts/install_stwo_run_and_prove.sh";

/// Errors that can occur when executing `stwo_run_and_prove`.
pub type StwoRunAndProveError = BinaryRunnerError;

/// Configuration for running `stwo_run_and_prove`.
#[derive(Debug, Clone)]
pub struct StwoRunAndProveConfig {
    /// Optional explicit path to the binary. If None, uses PATH lookup.
    pub binary_path: Option<PathBuf>,
    /// Timeout for the proving operation.
    pub timeout: Duration,
    /// Whether to verify the generated proof.
    pub verify: bool,
    /// Proof output format.
    pub proof_format: ProofFormat,
    /// Whether to always save debug data (even when there is no error).
    pub save_debug_data: bool,
    /// Directory for debug data output.
    pub debug_data_dir: Option<PathBuf>,
}

impl Default for StwoRunAndProveConfig {
    fn default() -> Self {
        Self {
            binary_path: None,
            timeout: DEFAULT_TIMEOUT,
            verify: false,
            proof_format: ProofFormat::CairoSerde,
            save_debug_data: false,
            debug_data_dir: None,
        }
    }
}

impl BinarySpec for StwoRunAndProveConfig {
    fn binary_name(&self) -> &str {
        DEFAULT_BINARY_NAME
    }

    fn env_var(&self) -> &str {
        STWO_RUN_AND_PROVE_PATH_ENV
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

/// Input for a `stwo_run_and_prove` invocation.
#[derive(Debug, Clone)]
pub struct StwoRunAndProveInput {
    /// Absolute path to the compiled program.
    pub program_path: PathBuf,
    /// Optional absolute path to the program input file.
    pub program_input_path: Option<PathBuf>,
    /// Optional absolute path to the prover parameters JSON file.
    pub prover_params_path: Option<PathBuf>,
    /// Absolute path where the generated proof will be saved.
    pub proof_output_path: PathBuf,
    /// Optional absolute path for the program output.
    pub program_output_path: Option<PathBuf>,
}

/// Runs `stwo_run_and_prove` with the given input and configuration.
///
/// # Arguments
///
/// * `input` - The input paths for the proving operation.
/// * `config` - Configuration for the proving operation.
///
/// # Returns
///
/// The output from the proving operation, or an error if it failed.
///
/// # Example
///
/// ```ignore
/// let input = StwoRunAndProveInput {
///     program_path: PathBuf::from("/path/to/program.json"),
///     program_input_path: Some(PathBuf::from("/path/to/input.json")),
///     prover_params_path: None,
///     proof_output_path: PathBuf::from("/path/to/proof.json"),
///     program_output_path: None,
/// };
///
/// let config = StwoRunAndProveConfig::default();
/// let output = run_stwo_run_and_prove(&input, &config).await?;
/// ```
pub async fn run_stwo_run_and_prove(
    input: &StwoRunAndProveInput,
    config: &StwoRunAndProveConfig,
) -> Result<BinaryOutput, StwoRunAndProveError> {
    // Resolve binary path.
    let binary_path = config.resolve_binary_path();

    // Build command.
    let mut command = Command::new(&binary_path);

    // Required arguments.
    command.arg("--program").arg(&input.program_path);
    command.arg("--proof_path").arg(&input.proof_output_path);

    // Optional arguments.
    if let Some(ref input_path) = input.program_input_path {
        command.arg("--program_input").arg(input_path);
    }

    if let Some(ref params_path) = input.prover_params_path {
        command.arg("--prover_params_json").arg(params_path);
    }

    if let Some(ref output_path) = input.program_output_path {
        command.arg("--program_output").arg(output_path);
    }

    // Configuration options.
    command.arg("--proof-format").arg(config.proof_format.as_str());

    if config.verify {
        command.arg("--verify");
    }

    if config.save_debug_data {
        command.arg("--save_debug_data");
    }

    if let Some(ref debug_dir) = config.debug_data_dir {
        command.arg("--debug_data_dir").arg(debug_dir);
    }

    // Set RUST_BACKTRACE for better error diagnostics.
    command.env("RUST_BACKTRACE", "1");

    // Log the command being executed (without sensitive data).
    tracing::info!(
        binary_path = %binary_path.display(),
        program = %input.program_path.display(),
        proof_output = %input.proof_output_path.display(),
        "Executing stwo_run_and_prove"
    );

    // Execute with timeout.
    let output = execute_with_timeout(&mut command, config).await?;

    // Check for errors.
    check_process_output(&output, config.binary_name())?;

    tracing::info!(
        proof_path = %input.proof_output_path.display(),
        "stwo_run_and_prove completed successfully"
    );

    Ok(BinaryOutput::from_output(&output))
}
