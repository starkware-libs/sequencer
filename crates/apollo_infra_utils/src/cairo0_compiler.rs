use std::path::{Path, PathBuf};
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
pub const EXPECTED_CAIRO0_VERSION: &str = "0.13.5";

/// The local python requirements used to determine the cairo0 compiler version.
#[cfg(any(test, feature = "testing"))]
pub(crate) static PIP_REQUIREMENTS_FILE: LazyLock<PathBuf> =
    LazyLock::new(|| resolve_project_relative_path("scripts/requirements.txt").unwrap());

#[derive(thiserror::Error, Debug)]
pub enum Cairo0CompilerVersionError {
    #[error("{0} not found.")]
    CompilerNotFound(String),
    #[error("{compiler} version is not correct: required {required}, got {existing}.")]
    IncorrectVersion { compiler: String, existing: String, required: String },
}

#[derive(thiserror::Error, Debug)]
pub enum Cairo0CompilerError {
    #[error(transparent)]
    Cairo0CompilerVersion(#[from] Cairo0CompilerVersionError),
    #[error("Cairo root path not found at {0:?}.")]
    CairoRootNotFound(PathBuf),
    #[error("Failed to compile the program. Error: {0}.")]
    CompileError(String),
    #[error("Invalid path unicode: {0:?}.")]
    InvalidPath(PathBuf),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("No file found at path {0:?}.")]
    SourceFileNotFound(PathBuf),
}

pub fn cairo0_compilers_correct_version() -> Result<(), Cairo0CompilerVersionError> {
    for compiler in [CAIRO0_COMPILE, STARKNET_COMPILE_DEPRECATED] {
        let version = match Command::new(compiler).arg("--version").output() {
            Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
            Err(error) => {
                return Err(Cairo0CompilerVersionError::CompilerNotFound(format!(
                    "Failed to get {compiler} version: {error}."
                )));
            }
        };
        if !version.trim().ends_with(EXPECTED_CAIRO0_VERSION) {
            return Err(Cairo0CompilerVersionError::IncorrectVersion {
                compiler: compiler.to_string(),
                existing: version,
                required: EXPECTED_CAIRO0_VERSION.to_string(),
            });
        }
    }

    Ok(())
}

/// Compile a Cairo0 program.
pub fn compile_cairo0_program(
    path_to_main: &Path,
    cairo_root_path: &Path,
) -> Result<Vec<u8>, Cairo0CompilerError> {
    cairo0_compilers_correct_version()?;
    if !path_to_main.exists() {
        return Err(Cairo0CompilerError::SourceFileNotFound(path_to_main.to_path_buf()));
    }
    if !cairo_root_path.exists() {
        return Err(Cairo0CompilerError::CairoRootNotFound(cairo_root_path.to_path_buf()));
    }
    let mut compile_command = Command::new(CAIRO0_COMPILE);
    compile_command.args([
        path_to_main
            .to_str()
            .ok_or(Cairo0CompilerError::InvalidPath(path_to_main.to_path_buf()))?,
        "--debug_info_with_source",
        "--cairo_path",
        cairo_root_path
            .to_str()
            .ok_or(Cairo0CompilerError::InvalidPath(cairo_root_path.to_path_buf()))?,
    ]);
    let compile_output = compile_command.output()?;

    // Verify output.
    if !compile_output.status.success() {
        return Err(Cairo0CompilerError::CompileError(
            String::from_utf8_lossy(&compile_output.stderr).trim().to_string(),
        ));
    }

    Ok(compile_output.stdout)
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
        Err(Cairo0CompilerVersionError::CompilerNotFound(_)) => {
            "no installed cairo-lang found".to_string()
        }
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
