//! Common utilities for executing external binaries.
//!
//! This module provides shared infrastructure for invoking external tools like
//! `stwo_run_and_prove` and `stwo_verify`, including:
//! - Binary path resolution
//! - Process execution with timeout
//! - Common error handling patterns
//! - Shared configuration types

use std::path::PathBuf;
use std::process::Output;
use std::time::Duration;

use apollo_infra_utils::path::resolve_project_relative_path;
use thiserror::Error;
use tokio::process::Command;

/// Default timeout for operations (10 minutes).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);

/// Maximum number of stderr lines to include in error messages.
pub const MAX_STDERR_LINES: usize = 20;

/// Proof output format, shared between proving and verification.
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
    pub fn as_str(&self) -> &'static str {
        match self {
            ProofFormat::Json => "json",
            ProofFormat::CairoSerde => "cairo-serde",
            ProofFormat::Binary => "binary",
        }
    }
}

/// Errors that can occur when executing an external binary.
#[derive(Debug, Error)]
pub enum BinaryRunnerError {
    /// The binary was not found at the configured path or in PATH.
    #[error(
        "{binary_name} binary not found. Either set {env_var} environment variable, add it to \
         PATH, or run: {install_script}"
    )]
    BinaryNotFound { binary_name: String, env_var: String, install_script: String },

    /// Failed to spawn the process.
    #[error("Failed to spawn {binary_name}: {source}")]
    SpawnError {
        binary_name: String,
        #[source]
        source: std::io::Error,
    },

    /// The process exited with a non-zero status.
    #[error(
        "{binary_name} failed with exit code {exit_code}.\nStderr (last {stderr_lines} \
         lines):\n{stderr}"
    )]
    ProcessFailed { binary_name: String, exit_code: i32, stderr: String, stderr_lines: usize },

    /// The process was killed by a signal.
    #[error("{binary_name} was killed by signal {signal}")]
    ProcessKilled { binary_name: String, signal: i32 },

    /// The process timed out.
    #[error(
        "{binary_name} timed out after {duration:?}. Consider increasing the timeout or checking \
         resource constraints."
    )]
    Timeout { binary_name: String, duration: Duration },

    /// IO error when reading output.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Specification for an external binary tool.
///
/// This trait defines the metadata needed to locate and run an external binary.
pub trait BinarySpec {
    /// The human-readable name of the binary (e.g., "stwo_run_and_prove").
    fn binary_name(&self) -> &str;

    /// The environment variable used to override the binary path.
    fn env_var(&self) -> &str;

    /// The relative path from repo root to the installed binary.
    fn install_relative_path(&self) -> &str;

    /// The install script path for error messages.
    fn install_script(&self) -> &str;

    /// Optional explicit path to the binary from configuration.
    fn configured_path(&self) -> Option<&PathBuf>;

    /// Timeout for the operation.
    fn timeout(&self) -> Duration;

    /// Returns the path to the locally installed binary (from install script), if it exists.
    fn get_local_install_path(&self) -> Option<PathBuf> {
        resolve_project_relative_path(self.install_relative_path()).ok()
    }

    /// Resolves the path to the binary using standard resolution order.
    ///
    /// Resolution order:
    /// 1. Explicit path from config (if provided)
    /// 2. Environment variable
    /// 3. Local install location: `<repo_root>/target/tools/<binary_name>`
    /// 4. Default binary name for PATH lookup
    fn resolve_binary_path(&self) -> PathBuf {
        // 1. Check explicit config.
        if let Some(path) = self.configured_path() {
            return path.clone();
        }

        // 2. Check environment variable.
        if let Ok(env_path) = std::env::var(self.env_var()) {
            return PathBuf::from(env_path);
        }

        // 3. Check local install location (from install script).
        if let Some(local_path) = self.get_local_install_path() {
            if local_path.exists() {
                return local_path;
            }
        }

        // 4. Fall back to PATH lookup.
        PathBuf::from(self.binary_name())
    }
}

/// Checks if the binary is available.
///
/// Returns the resolved path if found, or an error if not found.
pub async fn check_binary_available(spec: &impl BinarySpec) -> Result<PathBuf, BinaryRunnerError> {
    let binary_path = spec.resolve_binary_path();

    // Try running with --help to verify the binary exists and is executable.
    let result = Command::new(&binary_path).arg("--help").output().await;

    match result {
        Ok(output) if output.status.success() => Ok(binary_path),
        Ok(_) => {
            // Binary exists but --help failed (unusual).
            Ok(binary_path)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(BinaryRunnerError::BinaryNotFound {
                binary_name: spec.binary_name().to_string(),
                env_var: spec.env_var().to_string(),
                install_script: spec.install_script().to_string(),
            })
        }
        Err(e) => Err(BinaryRunnerError::SpawnError {
            binary_name: spec.binary_name().to_string(),
            source: e,
        }),
    }
}

/// Executes a command with timeout and returns the output.
///
/// This is a lower-level helper that handles the common timeout and spawn error logic.
pub async fn execute_with_timeout(
    command: &mut Command,
    spec: &impl BinarySpec,
) -> Result<Output, BinaryRunnerError> {
    tokio::time::timeout(spec.timeout(), command.output())
        .await
        .map_err(|_| BinaryRunnerError::Timeout {
            binary_name: spec.binary_name().to_string(),
            duration: spec.timeout(),
        })?
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                BinaryRunnerError::BinaryNotFound {
                    binary_name: spec.binary_name().to_string(),
                    env_var: spec.env_var().to_string(),
                    install_script: spec.install_script().to_string(),
                }
            } else {
                BinaryRunnerError::SpawnError {
                    binary_name: spec.binary_name().to_string(),
                    source: e,
                }
            }
        })
}

/// Process the output of a command execution and return an error if it failed.
///
/// On success, returns Ok(()). On failure, returns an appropriate error.
pub fn check_process_output(output: &Output, binary_name: &str) -> Result<(), BinaryRunnerError> {
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Check if killed by signal.
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = output.status.signal() {
            tracing::error!(
                signal,
                stderr = %stderr,
                "{} killed by signal", binary_name
            );
            return Err(BinaryRunnerError::ProcessKilled {
                binary_name: binary_name.to_string(),
                signal,
            });
        }
    }

    let exit_code = output.status.code().unwrap_or(-1);

    // Get last N lines of stderr for the error message.
    let stderr_lines: Vec<&str> = stderr.lines().collect();
    let stderr_tail = if stderr_lines.len() > MAX_STDERR_LINES {
        stderr_lines[stderr_lines.len() - MAX_STDERR_LINES..].join("\n")
    } else {
        stderr.clone()
    };

    tracing::error!(
        exit_code,
        stderr = %stderr_tail,
        "{} failed", binary_name
    );

    Err(BinaryRunnerError::ProcessFailed {
        binary_name: binary_name.to_string(),
        exit_code,
        stderr: stderr_tail,
        stderr_lines: MAX_STDERR_LINES.min(stderr_lines.len()),
    })
}

/// Standard output from a binary execution.
#[derive(Debug)]
pub struct BinaryOutput {
    /// Standard output from the process.
    pub stdout: String,
    /// Standard error from the process (may contain logs).
    pub stderr: String,
}

impl BinaryOutput {
    /// Create from process output.
    pub fn from_output(output: &Output) -> Self {
        Self {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    // Prevent concurrent env var mutation across tests.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    const TEST_ENV_VAR: &str = "TEST_BINARY_PATH";
    const TEST_BINARY_NAME: &str = "test_binary";
    const TEST_INSTALL_PATH: &str = "target/tools/test_binary";

    /// A simple test-only BinarySpec implementation.
    #[derive(Debug, Clone, Default)]
    struct TestConfig {
        binary_path: Option<PathBuf>,
    }

    impl BinarySpec for TestConfig {
        fn binary_name(&self) -> &str {
            TEST_BINARY_NAME
        }

        fn env_var(&self) -> &str {
            TEST_ENV_VAR
        }

        fn install_relative_path(&self) -> &str {
            TEST_INSTALL_PATH
        }

        fn install_script(&self) -> &str {
            "scripts/install_test_binary.sh"
        }

        fn configured_path(&self) -> Option<&PathBuf> {
            self.binary_path.as_ref()
        }

        fn timeout(&self) -> Duration {
            DEFAULT_TIMEOUT
        }
    }

    #[test]
    fn test_resolve_binary_path_from_config() {
        let custom_path = PathBuf::from("/custom/path/to/binary");
        let config = TestConfig { binary_path: Some(custom_path.clone()) };
        let path = config.resolve_binary_path();
        assert_eq!(path, custom_path);
    }

    #[test]
    fn test_resolve_binary_path_from_env() {
        let _lock = ENV_LOCK.lock().expect("env lock poisoned.");
        let config = TestConfig::default();
        let env_path = format!("/env/path/{}", config.binary_name());

        std::env::set_var(config.env_var(), &env_path);
        let path = config.resolve_binary_path();
        assert_eq!(path, PathBuf::from(&env_path));
        std::env::remove_var(config.env_var());
    }

    #[test]
    fn test_resolve_binary_path_default() {
        let _lock = ENV_LOCK.lock().expect("env lock poisoned.");
        let config = TestConfig::default();

        // Clear environment variable for this test.
        std::env::remove_var(config.env_var());

        let path = config.resolve_binary_path();

        // Should either find local install or fall back to PATH lookup.
        if let Some(local_path) = config.get_local_install_path() {
            if local_path.exists() {
                assert_eq!(path, local_path);
                return;
            }
        }
        assert_eq!(path, PathBuf::from(config.binary_name()));
    }

    #[test]
    fn test_get_local_install_path() {
        let config = TestConfig::default();
        // Should return a path ending with the expected relative path.
        if let Some(path) = config.get_local_install_path() {
            assert!(path.ends_with(config.install_relative_path()));
        }
        // It's OK if this returns None in some environments (e.g., if CARGO_MANIFEST_DIR is not
        // set).
    }
}
