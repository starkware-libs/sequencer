//! Wrapper module for executing the `stwo_run_and_prove` external binary.
//!
//! This module provides a robust interface for invoking the `stwo_run_and_prove` tool,
//! which runs a Cairo program and generates a Stwo proof for it.
//!
//! # Binary Resolution
//!
//! The binary is resolved in the following order:
//! 1. `STWO_RUN_AND_PROVE_PATH` environment variable (explicit path)
//! 2. Local install location: `<repo_root>/target/tools/stwo_run_and_prove`
//! 3. PATH lookup (preferred in Docker containers where it's at `/usr/local/bin/`)
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

use apollo_infra_utils::path::resolve_project_relative_path;
use thiserror::Error;
use tokio::process::Command;

/// Environment variable for overriding the `stwo_run_and_prove` binary path.
pub const STWO_RUN_AND_PROVE_PATH_ENV: &str = "STWO_RUN_AND_PROVE_PATH";

/// Default binary name for PATH lookup.
const DEFAULT_BINARY_NAME: &str = "stwo_run_and_prove";

/// Relative path from repo root to the installed binary.
const INSTALL_RELATIVE_PATH: &str = "target/tools/stwo_run_and_prove";

/// Default timeout for proving operations (10 minutes).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);

/// Errors that can occur when executing `stwo_run_and_prove`.
#[derive(Debug, Error)]
pub enum StwoRunAndProveError {
    /// The binary was not found at the configured path or in PATH.
    #[error(
        "stwo_run_and_prove binary not found. Either set {STWO_RUN_AND_PROVE_PATH_ENV} \
         environment variable, add it to PATH, or run: scripts/install_stwo_run_and_prove.sh"
    )]
    BinaryNotFound,

    /// Failed to spawn the process.
    #[error("Failed to spawn stwo_run_and_prove: {0}")]
    SpawnError(#[source] std::io::Error),

    /// The process exited with a non-zero status.
    #[error(
        "stwo_run_and_prove failed with exit code {exit_code}.\nStderr (last {stderr_lines} \
         lines):\n{stderr}"
    )]
    ProcessFailed { exit_code: i32, stderr: String, stderr_lines: usize },

    /// The process was killed by a signal.
    #[error("stwo_run_and_prove was killed by signal {signal}")]
    ProcessKilled { signal: i32 },

    /// The process timed out.
    #[error(
        "stwo_run_and_prove timed out after {duration:?}. Consider increasing the timeout or \
         checking resource constraints."
    )]
    Timeout { duration: Duration },

    /// IO error when reading output.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

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

/// Proof output format.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProofFormat {
    /// Standard JSON format.
    Json,
    /// Array of field elements serialized as hex strings. Compatible with `scarb execute`.
    #[default]
    CairoSerde,
    /// Binary format, additionally compressed to minimize the proof size.
    Binary,
}

impl ProofFormat {
    fn as_str(&self) -> &'static str {
        match self {
            ProofFormat::Json => "json",
            ProofFormat::CairoSerde => "cairo-serde",
            ProofFormat::Binary => "binary",
        }
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

/// Output from a successful `stwo_run_and_prove` invocation.
#[derive(Debug)]
pub struct StwoRunAndProveOutput {
    /// The path where the proof was written.
    pub proof_path: PathBuf,
    /// Standard output from the process.
    pub stdout: String,
    /// Standard error from the process (may contain logs).
    pub stderr: String,
}

/// Returns the path to the locally installed binary (from install script), if it exists.
pub fn get_local_install_path() -> Option<PathBuf> {
    resolve_project_relative_path(INSTALL_RELATIVE_PATH).ok()
}

/// Resolves the path to the `stwo_run_and_prove` binary.
///
/// Resolution order:
/// 1. Explicit path from config (if provided)
/// 2. `STWO_RUN_AND_PROVE_PATH` environment variable
/// 3. Local install location: `<repo_root>/target/tools/stwo_run_and_prove`
/// 4. Default binary name for PATH lookup
///
/// For option 3, the function checks if the file exists before returning.
/// If the local install doesn't exist, it falls back to PATH lookup.
pub fn resolve_binary_path(config: &StwoRunAndProveConfig) -> PathBuf {
    // 1. Check explicit config.
    if let Some(ref path) = config.binary_path {
        return path.clone();
    }

    // 2. Check environment variable.
    if let Ok(env_path) = std::env::var(STWO_RUN_AND_PROVE_PATH_ENV) {
        return PathBuf::from(env_path);
    }

    // 3. Check local install location (from install script).
    if let Some(local_path) = get_local_install_path() {
        if local_path.exists() {
            return local_path;
        }
    }

    // 4. Fall back to PATH lookup.
    PathBuf::from(DEFAULT_BINARY_NAME)
}

/// Checks if the `stwo_run_and_prove` binary is available.
///
/// Returns the resolved path if found, or an error if not found.
pub async fn check_binary_available(
    config: &StwoRunAndProveConfig,
) -> Result<PathBuf, StwoRunAndProveError> {
    let binary_path = resolve_binary_path(config);

    // Try running with --help to verify the binary exists and is executable.
    let result = Command::new(&binary_path).arg("--help").output().await;

    match result {
        Ok(output) if output.status.success() => Ok(binary_path),
        Ok(_) => {
            // Binary exists but --help failed (unusual).
            Ok(binary_path)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(StwoRunAndProveError::BinaryNotFound)
        }
        Err(e) => Err(StwoRunAndProveError::SpawnError(e)),
    }
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
) -> Result<StwoRunAndProveOutput, StwoRunAndProveError> {
    // Resolve binary path.
    let binary_path = resolve_binary_path(config);

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
    let output = tokio::time::timeout(config.timeout, command.output())
        .await
        .map_err(|_| StwoRunAndProveError::Timeout { duration: config.timeout })? // Handle timeout error
        .map_err(|e| { // Handle command execution error
            if e.kind() == std::io::ErrorKind::NotFound {
                StwoRunAndProveError::BinaryNotFound
            } else {
                StwoRunAndProveError::SpawnError(e)
            }
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Check exit status.
    if !output.status.success() {
        // Check if killed by signal.
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if let Some(signal) = output.status.signal() {
                tracing::error!(
                    signal,
                    stderr = %stderr,
                    "stwo_run_and_prove killed by signal"
                );
                return Err(StwoRunAndProveError::ProcessKilled { signal });
            }
        }

        let exit_code = output.status.code().unwrap_or(-1);

        // Get last N lines of stderr for the error message.
        const MAX_STDERR_LINES: usize = 20;
        let stderr_lines: Vec<&str> = stderr.lines().collect();
        let stderr_tail = if stderr_lines.len() > MAX_STDERR_LINES {
            stderr_lines[stderr_lines.len() - MAX_STDERR_LINES..].join("\n")
        } else {
            stderr.clone()
        };

        tracing::error!(
            exit_code,
            stderr = %stderr_tail,
            "stwo_run_and_prove failed"
        );

        return Err(StwoRunAndProveError::ProcessFailed {
            exit_code,
            stderr: stderr_tail,
            stderr_lines: MAX_STDERR_LINES.min(stderr_lines.len()),
        });
    }

    tracing::info!(
        proof_path = %input.proof_output_path.display(),
        "stwo_run_and_prove completed successfully"
    );

    Ok(StwoRunAndProveOutput { proof_path: input.proof_output_path.clone(), stdout, stderr })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    // Prevent concurrent env var mutation accross tests.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_resolve_binary_path_default() {
        let _lock = ENV_LOCK.lock().expect("env lock poisoned.");
        // Clear environment variable for this test.
        std::env::remove_var(STWO_RUN_AND_PROVE_PATH_ENV);

        let config = StwoRunAndProveConfig::default();
        let path = resolve_binary_path(&config);

        // Should either find local install or fall back to PATH lookup.
        if let Some(local_path) = get_local_install_path() {
            if local_path.exists() {
                assert_eq!(path, local_path);
                return;
            }
        }
        assert_eq!(path, PathBuf::from(DEFAULT_BINARY_NAME));
    }

    #[test]
    fn test_get_local_install_path() {
        // Should return a path ending with the expected relative path.
        if let Some(path) = get_local_install_path() {
            assert!(path.ends_with("target/tools/stwo_run_and_prove"));
        }
        // It's OK if this returns None in some environments (e.g., if CARGO_MANIFEST_DIR is not
        // set).
    }

    #[test]
    fn test_resolve_binary_path_from_config() {
        let config = StwoRunAndProveConfig {
            binary_path: Some(PathBuf::from("/custom/path/stwo_run_and_prove")),
            ..Default::default()
        };
        let path = resolve_binary_path(&config);
        assert_eq!(path, PathBuf::from("/custom/path/stwo_run_and_prove"));
    }

    #[test]
    fn test_resolve_binary_path_from_env() {
        let _lock = ENV_LOCK.lock().expect("env lock poisoned.");
        std::env::set_var(STWO_RUN_AND_PROVE_PATH_ENV, "/env/path/stwo_run_and_prove");
        let config = StwoRunAndProveConfig::default();
        let path = resolve_binary_path(&config);
        assert_eq!(path, PathBuf::from("/env/path/stwo_run_and_prove"));
        std::env::remove_var(STWO_RUN_AND_PROVE_PATH_ENV);
    }
}
