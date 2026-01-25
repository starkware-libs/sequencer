use std::io::{Read, Write};
use std::process::Command;

use crate::cairo0_compiler::{
    cairo0_script_correct_version,
    Cairo0Script,
    Cairo0ScriptVersionError,
};
use crate::path::resolve_project_relative_path;

/// Verifies that the required Cairo0 compiler is available; panics if unavailable.
/// For use in tests only. If cairo0 compiler verification is required in business logic, use
/// `crate::cairo0_compiler::cairo0_scripts_correct_version` instead.
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

    // Run isort on the formatted file.
    // Note: We use the `format_output` stdout but since cairo-format writes to stdout,
    // we need to write that back to the temp file for isort to process.
    let formatted_content = format_output.stdout;
    let mut temp_file_formatted = tempfile::NamedTempFile::new().unwrap();
    temp_file_formatted.write_all(&formatted_content).unwrap();

    // Run isort.
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
    isort_command.arg(temp_file_formatted.path().to_str().unwrap());

    let isort_output = isort_command.output().unwrap();
    let isort_stderr = String::from_utf8(isort_output.stderr).unwrap();
    assert!(isort_output.status.success(), "{isort_stderr}");

    // Return formatted file.
    let mut final_content = String::new();
    std::fs::File::open(temp_file_formatted.path())
        .unwrap()
        .read_to_string(&mut final_content)
        .unwrap();
    final_content
}

fn enter_venv_instructions(script_type: &Cairo0Script) -> String {
    format!(
        r#"
python3 -m venv sequencer_venv
. sequencer_venv/bin/activate
pip install -r {:?}"#,
        script_type.requirements_file_path()
    )
}
