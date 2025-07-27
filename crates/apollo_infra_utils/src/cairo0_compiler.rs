#[cfg(any(test, feature = "testing"))]
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::LazyLock;

use crate::path::resolve_project_relative_path;

#[cfg(test)]
#[path = "cairo0_compiler_test.rs"]
pub mod test;

#[derive(Debug, Eq, PartialEq)]
pub struct CairoLangVersion<'a>(pub &'a str);

pub const EXPECTED_CAIRO0_STARKNET_COMPILE_VERSION: CairoLangVersion<'static> =
    CairoLangVersion("0.14.0a1");
pub const EXPECTED_CAIRO0_VERSION: CairoLangVersion<'static> = CairoLangVersion("0.14.0.1");

/// The local python requirements used to determine the cairo0 compiler version.
pub(crate) static PIP_REQUIREMENTS_FILE: LazyLock<PathBuf> =
    LazyLock::new(|| resolve_project_relative_path("scripts/requirements.txt").unwrap());
pub(crate) static STARKNET_DEPRECATED_COMPILE_REQUIREMENTS_FILE: LazyLock<PathBuf> =
    LazyLock::new(|| {
        resolve_project_relative_path(
            "crates/blockifier_test_utils/resources/blockifier-test-utils-requirements.txt",
        )
        .unwrap()
    });

fn enter_venv_instructions(script_type: &Cairo0Script) -> String {
    format!(
        r#"
python3 -m venv sequencer_venv
. sequencer_venv/bin/activate
pip install -r {:?}"#,
        script_type.requirements_file_path()
    )
}

#[derive(Clone, Copy, Debug)]
pub enum Cairo0Script {
    Compile,
    Format,
    StarknetCompileDeprecated,
}

impl Cairo0Script {
    pub fn script_name(&self) -> &'static str {
        match self {
            Self::Compile => "cairo-compile",
            Self::Format => "cairo-format",
            Self::StarknetCompileDeprecated => "starknet-compile-deprecated",
        }
    }

    pub fn required_version(&self) -> CairoLangVersion<'static> {
        match self {
            Self::Compile | Self::Format => EXPECTED_CAIRO0_VERSION,
            Self::StarknetCompileDeprecated => EXPECTED_CAIRO0_STARKNET_COMPILE_VERSION,
        }
    }

    pub fn requirements_file_path(&self) -> &PathBuf {
        match self {
            Self::Compile | Self::Format => &PIP_REQUIREMENTS_FILE,
            Self::StarknetCompileDeprecated => &STARKNET_DEPRECATED_COMPILE_REQUIREMENTS_FILE,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Cairo0ScriptVersionError {
    #[error(
        "{script:?} version is not correct: required {required}, got {existing}. Are you in the \
         venv? If not, run the following commands:\n{}",
        enter_venv_instructions(script)
    )]
    IncorrectVersion { script: Cairo0Script, existing: String, required: String },
    #[error(
        "{error}. Are you in the venv? If not, run the following commands:\n{}",
        enter_venv_instructions(script)
    )]
    CompilerNotFound { script: Cairo0Script, error: String },
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

pub fn cairo0_script_correct_version(
    script_type: &Cairo0Script,
) -> Result<(), Cairo0ScriptVersionError> {
    let expected_version = script_type.required_version();
    let script = script_type.script_name();
    let version = match Command::new(script).arg("--version").output() {
        Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
        Err(error) => {
            return Err(Cairo0ScriptVersionError::CompilerNotFound {
                script: *script_type,
                error: format!("Failed to get {script} version: {error}."),
            });
        }
    };
    if CairoLangVersion(version.trim().replace("==", " ").split(" ").nth(1).ok_or(
        Cairo0ScriptVersionError::CompilerNotFound {
            script: *script_type,
            error: format!("No {script} version found."),
        },
    )?) != expected_version
    {
        return Err(Cairo0ScriptVersionError::IncorrectVersion {
            script: *script_type,
            existing: version,
            required: expected_version.0.to_string(),
        });
    }

    Ok(())
}

/// Compile a Cairo0 program.
pub fn compile_cairo0_program(
    path_to_main: PathBuf,
    cairo_root_path: PathBuf,
) -> Result<Vec<u8>, Cairo0CompilerError> {
    let script_type = Cairo0Script::Compile;
    cairo0_script_correct_version(&script_type)?;
    if !path_to_main.exists() {
        return Err(Cairo0CompilerError::SourceFileNotFound(path_to_main));
    }
    if !cairo_root_path.exists() {
        return Err(Cairo0CompilerError::CairoRootNotFound(cairo_root_path));
    }
    let mut compile_command = Command::new(script_type.script_name());
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
pub fn verify_cairo0_compiler_deps(script_type: &Cairo0Script) {
    if let Err(verification_error) = cairo0_script_correct_version(script_type) {
        let error = match verification_error {
            Cairo0ScriptVersionError::CompilerNotFound { error, .. } => {
                format!("No installed cairo-lang found. Original error: {error}.")
            }
            Cairo0ScriptVersionError::IncorrectVersion { existing, script, .. } => {
                format!("Installed {script:?} version: {existing}")
            }
        };
        panic!(
            "{script_type:?} script not found or of incorrect version ({error}). Please enter a \
             venv and rerun the test:\n{}",
            enter_venv_instructions(script_type)
        )
    }
}

/// Runs the Cairo0 formatter on the input source code.
#[cfg(any(test, feature = "testing"))]
pub fn cairo0_format(unformatted: &String) -> String {
    let script_type = Cairo0Script::Format;
    verify_cairo0_compiler_deps(&script_type);

    // Dump string to temporary file.
    let mut temp_file = tempfile::NamedTempFile::new().unwrap();
    temp_file.write_all(unformatted.as_bytes()).unwrap();

    // Run formatter.
    let mut command = Command::new(script_type.script_name());
    command.arg(temp_file.path().to_str().unwrap());
    let format_output = command.output().unwrap();
    let stderr_output = String::from_utf8(format_output.stderr).unwrap();
    assert!(format_output.status.success(), "{stderr_output}");

    // Return formatted file.
    String::from_utf8_lossy(format_output.stdout.as_slice()).to_string()
}
