//! Wrapper module for executing the `stwo_verify` external binary.
//!
//! This module provides a robust interface for invoking the `stwo_verify` tool,
//! which verifies a Stwo Cairo proof.
//!
//! # Binary Resolution
//!
//! The binary is resolved in the following order:
//! 1. `STWO_VERIFY_PATH` environment variable (explicit path)
//! 2. Local install location: `<repo_root>/target/tools/stwo_verify`
//! 3. PATH lookup (preferred in Docker containers where it's at `/usr/local/bin/`)
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

use apollo_infra_utils::path::resolve_project_relative_path;
use thiserror::Error;
use tokio::process::Command;

/// Environment variable for overriding the `stwo_verify` binary path.
pub const STWO_VERIFY_PATH_ENV: &str = "STWO_VERIFY_PATH";

/// Default binary name for PATH lookup.
const DEFAULT_BINARY_NAME: &str = "stwo_verify";

/// Relative path from repo root to the installed binary.
const INSTALL_RELATIVE_PATH: &str = "target/tools/stwo_verify";

/// Default timeout for verification operations (10 minutes).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);

/// Errors that can occur when executing `stwo_verify`.
#[derive(Debug, Error)]
pub enum StwoVerifyError {
    /// The binary was not found at the configured path or in PATH.
    #[error(
        "stwo_verify binary not found. Either set {STWO_VERIFY_PATH_ENV} environment variable, \
         add it to PATH, or run: scripts/install_stwo_verify.sh"
    )]
    BinaryNotFound,

    /// Failed to spawn the process.
    #[error("Failed to spawn stwo_verify: {0}")]
    SpawnError(#[source] std::io::Error),

    /// The process exited with a non-zero status.
    #[error(
        "stwo_verify failed with exit code {exit_code}.\nStderr (last {stderr_lines} \
         lines):\n{stderr}"
    )]
    ProcessFailed { exit_code: i32, stderr: String, stderr_lines: usize },

    /// The process was killed by a signal.
    #[error("stwo_verify was killed by signal {signal}")]
    ProcessKilled { signal: i32 },

    /// The process timed out.
    #[error(
        "stwo_verify timed out after {duration:?}. Consider increasing the timeout or checking \
         resource constraints."
    )]
    Timeout { duration: Duration },

    /// IO error when reading output.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

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

/// Proof format for `stwo_verify`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProofFormat {
    /// Standard JSON format.
    Json,
    /// Array of field elements serialized as hex strings.
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

/// Output from a successful `stwo_verify` invocation.
#[derive(Debug)]
pub struct StwoVerifyOutput {
    /// Standard output from the process.
    pub stdout: String,
    /// Standard error from the process (may contain logs).
    pub stderr: String,
}

/// Returns the path to the locally installed binary (from install script), if it exists.
pub fn get_local_install_path() -> Option<PathBuf> {
    resolve_project_relative_path(INSTALL_RELATIVE_PATH).ok()
}

/// Resolves the path to the `stwo_verify` binary.
///
/// Resolution order:
/// 1. Explicit path from config (if provided)
/// 2. `STWO_VERIFY_PATH` environment variable
/// 3. Local install location: `<repo_root>/target/tools/stwo_verify`
/// 4. Default binary name for PATH lookup
///
/// For option 3, the function checks if the file exists before returning.
/// If the local install doesn't exist, it falls back to PATH lookup.
pub fn resolve_binary_path(config: &StwoVerifyConfig) -> PathBuf {
    // 1. Check explicit config.
    if let Some(ref path) = config.binary_path {
        return path.clone();
    }

    // 2. Check environment variable.
    if let Ok(env_path) = std::env::var(STWO_VERIFY_PATH_ENV) {
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

/// Checks if the `stwo_verify` binary is available.
///
/// Returns the resolved path if found, or an error if not found.
pub async fn check_binary_available(config: &StwoVerifyConfig) -> Result<PathBuf, StwoVerifyError> {
    let binary_path = resolve_binary_path(config);

    // Try running with --help to verify the binary exists and is executable.
    let result = Command::new(&binary_path).arg("--help").output().await;

    match result {
        Ok(output) if output.status.success() => Ok(binary_path),
        Ok(_) => {
            // Binary exists but --help failed (unusual).
            Ok(binary_path)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(StwoVerifyError::BinaryNotFound),
        Err(e) => Err(StwoVerifyError::SpawnError(e)),
    }
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
) -> Result<StwoVerifyOutput, StwoVerifyError> {
    // Resolve binary path.
    let binary_path = resolve_binary_path(config);

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
    let output = tokio::time::timeout(config.timeout, command.output())
        .await
        .map_err(|_| StwoVerifyError::Timeout { duration: config.timeout })?
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StwoVerifyError::BinaryNotFound
            } else {
                StwoVerifyError::SpawnError(e)
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
                    "stwo_verify killed by signal"
                );
                return Err(StwoVerifyError::ProcessKilled { signal });
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
            "stwo_verify failed"
        );

        return Err(StwoVerifyError::ProcessFailed {
            exit_code,
            stderr: stderr_tail,
            stderr_lines: MAX_STDERR_LINES.min(stderr_lines.len()),
        });
    }

    tracing::info!("stwo_verify completed successfully");

    Ok(StwoVerifyOutput { stdout, stderr })
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
        std::env::remove_var(STWO_VERIFY_PATH_ENV);

        let config = StwoVerifyConfig::default();
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
            assert!(path.ends_with("target/tools/stwo_verify"));
        }
        // It's OK if this returns None in some environments (e.g., if CARGO_MANIFEST_DIR is not
        // set).
    }

    #[test]
    fn test_resolve_binary_path_from_config() {
        let config = StwoVerifyConfig {
            binary_path: Some(PathBuf::from("/custom/path/stwo_verify")),
            ..Default::default()
        };
        let path = resolve_binary_path(&config);
        assert_eq!(path, PathBuf::from("/custom/path/stwo_verify"));
    }

    #[test]
    fn test_resolve_binary_path_from_env() {
        let _lock = ENV_LOCK.lock().expect("env lock poisoned.");
        std::env::set_var(STWO_VERIFY_PATH_ENV, "/env/path/stwo_verify");
        let config = StwoVerifyConfig::default();
        let path = resolve_binary_path(&config);
        assert_eq!(path, PathBuf::from("/env/path/stwo_verify"));
        std::env::remove_var(STWO_VERIFY_PATH_ENV);
    }
}
