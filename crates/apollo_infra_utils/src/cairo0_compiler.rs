use std::path::PathBuf;
use std::process::Command;
use std::sync::LazyLock;

use crate::path::resolve_project_relative_path;

#[cfg(test)]
#[path = "cairo0_compiler_test.rs"]
pub mod test;

pub const STARKNET_COMPILE_DEPRECATED: &str = "starknet-compile-deprecated";
pub const CAIRO0_COMPILE: &str = "cairo-compile";
pub const CAIRO0_FORMAT: &str = "cairo-format";
pub const EXPECTED_CAIRO0_VERSION: &str = "0.14.0a1";

/// The local python requirements used to determine the cairo0 compiler version.
pub(crate) static PIP_REQUIREMENTS_FILE: LazyLock<PathBuf> =
    LazyLock::new(|| resolve_project_relative_path("scripts/requirements.txt").unwrap());

static ENTER_VENV_INSTRUCTIONS: LazyLock<String> = LazyLock::new(|| {
    format!(
        r#"
python3 -m venv sequencer_venv
. sequencer_venv/bin/activate
pip install -r {:#?}"#,
        *PIP_REQUIREMENTS_FILE
    )
});

#[derive(thiserror::Error, Debug)]
pub enum Cairo0ScriptVersionError {
    #[error(
        "{script} version is not correct: required {required}, got {existing}. Are you in the \
         venv? If not, run the following commands:\n{}", *ENTER_VENV_INSTRUCTIONS
    )]
    IncorrectVersion { script: String, existing: String, required: String },
    #[error(
        "{0}. Are you in the venv? If not, run the following commands:\n{}",
        *ENTER_VENV_INSTRUCTIONS
    )]
    CompilerNotFound(String),
}

#[derive(thiserror::Error, Debug)]
pub enum Cairo0CompilerError {
    #[error(transparent)]
    Cairo0CompilerVersion(#[from] Cairo0ScriptVersionError),
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

pub fn cairo0_scripts_correct_version() -> Result<(), Cairo0ScriptVersionError> {
    for script in [CAIRO0_COMPILE, CAIRO0_FORMAT, STARKNET_COMPILE_DEPRECATED] {
        let version = match Command::new(script).arg("--version").output() {
            Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
            Err(error) => {
                return Err(Cairo0ScriptVersionError::CompilerNotFound(format!(
                    "Failed to get {script} version: {error}."
                )));
            }
        };
        if version.trim().replace("==", " ").split(" ").nth(1).ok_or(
            Cairo0ScriptVersionError::CompilerNotFound("No script version found.".to_string()),
        )? != EXPECTED_CAIRO0_VERSION
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

/// Compile a Cairo0 program.
pub fn compile_cairo0_program(
    path_to_main: PathBuf,
    cairo_root_path: PathBuf,
) -> Result<Vec<u8>, Cairo0CompilerError> {
    cairo0_scripts_correct_version()?;
    if !path_to_main.exists() {
        return Err(Cairo0CompilerError::SourceFileNotFound(path_to_main));
    }
    if !cairo_root_path.exists() {
        return Err(Cairo0CompilerError::CairoRootNotFound(cairo_root_path));
    }
    let mut compile_command = Command::new(CAIRO0_COMPILE);
    compile_command.args([
        path_to_main.to_str().ok_or(Cairo0CompilerError::InvalidPath(path_to_main.clone()))?,
        "--debug_info_with_source",
        "--cairo_path",
        cairo_root_path
            .to_str()
            .ok_or(Cairo0CompilerError::InvalidPath(cairo_root_path.clone()))?,
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
/// `crate::cairo0_compiler::cairo0_scripts_correct_version` instead.
#[cfg(any(test, feature = "testing"))]
pub fn verify_cairo0_compiler_deps() {
    let specific_error = match cairo0_scripts_correct_version() {
        Ok(_) => {
            return;
        }
        Err(Cairo0ScriptVersionError::CompilerNotFound(_)) => {
            "no installed cairo-lang found".to_string()
        }
        Err(Cairo0ScriptVersionError::IncorrectVersion { existing, .. }) => {
            format!("installed version: {existing}")
        }
    };

    panic!(
        "cairo-lang version {EXPECTED_CAIRO0_VERSION} not found ({specific_error}). Please enter \
         a venv and rerun the test:\n{}",
        *ENTER_VENV_INSTRUCTIONS
    );
}
