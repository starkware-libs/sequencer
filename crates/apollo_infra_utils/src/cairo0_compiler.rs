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
pub const CAIRO0_FORMAT: &str = "cairo-format";
pub const EXPECTED_CAIRO0_VERSION: &str = "0.14.0a1";

/// The local python requirements used to determine the cairo0 compiler version.
#[cfg(any(test, feature = "testing"))]
pub(crate) static PIP_REQUIREMENTS_FILE: LazyLock<PathBuf> =
    LazyLock::new(|| resolve_project_relative_path("scripts/requirements.txt").unwrap());

#[derive(thiserror::Error, Debug)]
pub enum Cairo0ScriptVersionError {
    #[error("{script} version is not correct: required {required}, got {existing}.")]
    IncorrectVersion { script: String, existing: String, required: String },
    #[error("{0} not found.")]
    NotFound(String),
}

pub fn cairo0_scripts_correct_version() -> Result<(), Cairo0ScriptVersionError> {
    for script in [CAIRO0_COMPILE, CAIRO0_FORMAT, STARKNET_COMPILE_DEPRECATED] {
        let version = match Command::new(script).arg("--version").output() {
            Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
            Err(error) => {
                return Err(Cairo0ScriptVersionError::NotFound(format!(
                    "Failed to get {script} version: {error}."
                )));
            }
        };
        if version
            .trim()
            .replace("==", " ")
            .split(" ")
            .nth(1)
            .ok_or(Cairo0ScriptVersionError::NotFound("No script version found.".to_string()))?
            != EXPECTED_CAIRO0_VERSION
        {
            return Err(Cairo0ScriptVersionError::IncorrectVersion {
                script: script.to_string(),
                existing: version,
                required: EXPECTED_CAIRO0_VERSION.to_string(),
            });
        }
    }

    Ok(())
}

/// Verifies that the required Cairo0 compiler is available; panics if unavailable.
/// For use in tests only. If cairo0 compiler verification is required in business logic, use
/// `crate::cairo0_compiler::cairo0_scripts_correct_version` instead.
#[cfg(any(test, feature = "testing"))]
pub fn verify_cairo0_compiler_deps() {
    let specific_error = match cairo0_scripts_correct_version() {
        Ok(_) => {
            return;
        }
        Err(Cairo0ScriptVersionError::NotFound(_)) => "no installed cairo-lang found".to_string(),
        Err(Cairo0ScriptVersionError::IncorrectVersion { existing, .. }) => {
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
