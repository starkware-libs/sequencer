#[cfg(any(test, feature = "testing"))]
use std::path::PathBuf;
use std::process::Command;
#[cfg(any(test, feature = "testing"))]
use std::sync::LazyLock;

#[cfg(any(test, feature = "testing"))]
use crate::path::resolve_project_relative_path;

#[cfg(test)]
#[path = "cairo0_compiler_test.rs"]
pub mod test;

pub const STARKNET_COMPILE_DEPRECATED: &str = "starknet-compile-deprecated";
pub const CAIRO0_COMPILE: &str = "cairo-compile";
pub const EXPECTED_CAIRO0_VERSION: &str = "0.14.0a1";

/// The local python requirements used to determine the cairo0 compiler version.
#[cfg(any(test, feature = "testing"))]
pub(crate) static PIP_REQUIREMENTS_FILE: LazyLock<PathBuf> =
    LazyLock::new(|| resolve_project_relative_path("scripts/requirements.txt").unwrap());

#[derive(thiserror::Error, Debug)]
pub enum Cairo0CompilerVersionError {
    #[error("{compiler} version is not correct: required {required}, got {existing}.")]
    IncorrectVersion { compiler: String, existing: String, required: String },
    #[error("{0} not found.")]
    NotFound(String),
}

pub fn cairo0_compilers_correct_version() -> Result<(), Cairo0CompilerVersionError> {
    for compiler in [CAIRO0_COMPILE, STARKNET_COMPILE_DEPRECATED] {
        let version = match Command::new(compiler).arg("--version").output() {
            Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
            Err(error) => {
                return Err(Cairo0CompilerVersionError::NotFound(format!(
                    "Failed to get {compiler} version: {error}."
                )));
            }
        };
        if version
            .trim()
            .replace("==", " ")
            .split(" ")
            .nth(1)
            .ok_or(Cairo0CompilerVersionError::NotFound("No compiler version found.".to_string()))?
            != EXPECTED_CAIRO0_VERSION
        {
            return Err(Cairo0CompilerVersionError::IncorrectVersion {
                compiler: compiler.to_string(),
                existing: version,
                required: EXPECTED_CAIRO0_VERSION.to_string(),
            });
        }
    }

    Ok(())
}

/// Verifies that the required Cairo0 compiler is available; panics if unavailable.
/// For use in tests only. If cairo0 compiler verification is required in business logic, use
/// `crate::cairo0_compiler::cairo0_compilers_correct_version` instead.
#[cfg(any(test, feature = "testing"))]
pub fn verify_cairo0_compiler_deps() {
    let specific_error = match cairo0_compilers_correct_version() {
        Ok(_) => {
            return;
        }
        Err(Cairo0CompilerVersionError::NotFound(_)) => "no installed cairo-lang found".to_string(),
        Err(Cairo0CompilerVersionError::IncorrectVersion { existing, .. }) => {
            format!("installed version: {existing}")
        }
    };

    panic!(
        "cairo-lang version {EXPECTED_CAIRO0_VERSION} not found ({specific_error}). Run the \
         following commands (enter a python venv and install dependencies) and retry:\npython -m \
         venv sequencer_venv\n. sequencer_venv/bin/activate\npip install -r {:?}",
        PIP_REQUIREMENTS_FILE.to_str().expect("Path to requirements.txt is valid unicode.")
    );
}
