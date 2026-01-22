#[cfg(any(test, feature = "testing"))]
use std::collections::HashMap;
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
    CairoLangVersion("0.14.0.1");
pub const EXPECTED_CAIRO0_VERSION: CairoLangVersion<'static> = CairoLangVersion("0.14.1a0");

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

/// Builds a string containing all lines except those in the excluded range.
#[cfg(any(test, feature = "testing"))]
fn build_rest_of_file(lines: &[&str], exclude_start: usize, exclude_end: usize) -> String {
    lines
        .iter()
        .enumerate()
        .filter(|(idx, _)| *idx < exclude_start || *idx > exclude_end)
        .map(|(_, l)| *l)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Filters import names, keeping only those that are used in the given code.
#[cfg(any(test, feature = "testing"))]
fn filter_used_imports<'a>(import_names: &[&'a str], rest_of_file: &str) -> Vec<&'a str> {
    import_names.iter().filter(|name| is_import_used(name, rest_of_file)).copied().collect()
}

/// Removes unused imports from Cairo0 source code.
/// Returns the content with unused imports removed.
#[cfg(any(test, feature = "testing"))]
pub fn remove_unused_cairo0_imports(content: &str) -> String {
    use regex::Regex;

    let lines: Vec<&str> = content.lines().collect();
    let mut result_lines: Vec<String> = Vec::new();
    let mut i = 0;

    // Regex to match import lines: `from X import Y` or `from X import (`
    let single_import_re = Regex::new(r"^from\s+\S+\s+import\s+(.+)$").unwrap();
    let multi_import_start_re = Regex::new(r"^from\s+\S+\s+import\s+\($").unwrap();

    while i < lines.len() {
        let line = lines[i];

        // Check if this is a multi-line import (starts with `from X import (`)
        if multi_import_start_re.is_match(line) {
            let start_line = i;
            let mut import_names: Vec<&str> = Vec::new();
            i += 1;

            // Collect all imported names until we hit the closing `)`
            while i < lines.len() && !lines[i].trim().starts_with(')') {
                let import_line = lines[i].trim();
                // Extract name (remove trailing comma if present)
                let name = import_line.trim_end_matches(',').trim();
                if !name.is_empty() {
                    import_names.push(name);
                }
                i += 1;
            }
            let end_line = i;

            let rest_of_file = build_rest_of_file(&lines, start_line, end_line);
            let used_imports = filter_used_imports(&import_names, &rest_of_file);

            if used_imports.is_empty() {
                // All imports unused, skip the entire import block
            } else if used_imports.len() == import_names.len() {
                // All imports used, keep as-is
                for j in start_line..=end_line {
                    result_lines.push(lines[j].to_string());
                }
            } else if used_imports.len() == 1 {
                // Only one import used, convert to single-line import
                let from_part = line.trim_end_matches('(').trim_end();
                result_lines.push(format!("{} {}", from_part, used_imports[0]));
            } else {
                // Multiple imports used, keep multi-line format with only used imports
                result_lines.push(line.to_string());
                for name in &used_imports {
                    result_lines.push(format!("    {},", name));
                }
                result_lines.push(")".to_string());
            }
            i = end_line + 1;
            continue;
        }

        // Check if this is a single-line import
        if let Some(caps) = single_import_re.captures(line) {
            let imports_part = caps.get(1).unwrap().as_str();

            // Handle comma-separated imports on a single line: `from X import A, B, C`
            let import_names: Vec<&str> =
                imports_part.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();

            let rest_of_file = build_rest_of_file(&lines, i, i);
            let used_imports = filter_used_imports(&import_names, &rest_of_file);

            if used_imports.is_empty() {
                // All imports unused, skip this line
            } else if used_imports.len() == import_names.len() {
                // All imports used, keep as-is
                result_lines.push(line.to_string());
            } else {
                // Some imports used, reconstruct the line
                let from_part = line.split(" import ").next().unwrap_or("from unknown_module");
                result_lines.push(format!("{} import {}", from_part, used_imports.join(", ")));
            }
            i += 1;
            continue;
        }

        // Not an import line, keep as-is
        result_lines.push(line.to_string());
        i += 1;
    }

    result_lines.join("\n") + if content.ends_with('\n') { "\n" } else { "" }
}

/// Extracts the name that will be used in code from an import item.
/// Handles `X as Y` syntax by returning Y, otherwise returns X.
#[cfg(any(test, feature = "testing"))]
fn get_imported_name(import_item: &str) -> &str {
    // Handle "X as Y" - we need to search for Y in the code
    if let Some(as_pos) = import_item.find(" as ") {
        import_item[as_pos + 4..].trim()
    } else {
        import_item.trim()
    }
}

/// Checks if an import item is used in the given code.
/// Handles `X as Y` syntax by checking if Y is used.
/// Uses word boundary matching to avoid false positives.
#[cfg(any(test, feature = "testing"))]
fn is_import_used(import_item: &str, code: &str) -> bool {
    use regex::Regex;
    let name_to_search = get_imported_name(import_item);
    // Match identifier as a whole word (not part of another identifier)
    let pattern = format!(r"\b{}\b", regex::escape(name_to_search));
    let re = Regex::new(&pattern).unwrap();
    re.is_match(code)
}

/// Runs the Cairo0 formatter on the input source code.
/// For processing multiple files, use `cairo0_format_batch` for better performance.
#[cfg(any(test, feature = "testing"))]
pub fn cairo0_format(unformatted: &String) -> String {
    let files: HashMap<String, &String> =
        [("single_file.cairo".to_string(), unformatted)].into_iter().collect();
    let results = cairo0_format_batch(files);
    results.into_values().next().unwrap()
}

/// Runs the Cairo0 formatter on multiple files in a single batch.
/// This is much faster than calling `cairo0_format` multiple times because it only
/// spawns the external cairo-format and isort processes once.
///
/// Takes a map of (filename -> content) and returns a map of (filename -> formatted_content).
#[cfg(any(test, feature = "testing"))]
pub fn cairo0_format_batch<S: AsRef<str>>(files: HashMap<String, S>) -> HashMap<String, String> {
    if files.is_empty() {
        return HashMap::new();
    }

    let script_type = Cairo0Script::Format;
    verify_cairo0_compiler_deps(&script_type);

    // Create a temporary directory and write all files to it.
    let temp_dir = tempfile::TempDir::new().unwrap();
    let mut file_paths: Vec<PathBuf> = Vec::with_capacity(files.len());

    for (filename, content) in &files {
        let file_path = temp_dir.path().join(filename);
        // Create parent directories if needed.
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&file_path, content.as_ref()).unwrap();
        file_paths.push(file_path);
    }

    // Run cairo-format on all files at once with -i (in-place).
    let mut format_command = Command::new(script_type.script_name());
    format_command.arg("-i");
    for path in &file_paths {
        format_command.arg(path);
    }
    let format_output = format_command.output().unwrap();
    let stderr_output = String::from_utf8_lossy(&format_output.stderr);
    assert!(format_output.status.success(), "cairo-format failed: {stderr_output}");

    // Run isort on all files at once.
    let mut isort_command = Command::new("isort");
    let isort_config_path = resolve_project_relative_path(".isort.cfg").unwrap();
    isort_command.args([
        "--settings-file",
        isort_config_path.to_str().unwrap(),
        "--lai",
        "1",
        "-m",
        "3",
        // Checks that imports with parentheses include a trailing comma.
        "--tc",
    ]);
    for path in &file_paths {
        isort_command.arg(path);
    }
    let isort_output = isort_command.output().unwrap();
    let isort_stderr = String::from_utf8_lossy(&isort_output.stderr);
    assert!(isort_output.status.success(), "isort failed: {isort_stderr}");

    // Read back all files and remove unused imports.
    let mut results = HashMap::with_capacity(files.len());
    for (filename, _) in files {
        let file_path = temp_dir.path().join(&filename);
        let content = std::fs::read_to_string(&file_path).unwrap();
        let formatted = remove_unused_cairo0_imports(&content);
        results.insert(filename, formatted);
    }

    results
}
