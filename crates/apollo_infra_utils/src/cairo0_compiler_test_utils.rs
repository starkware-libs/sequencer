use std::io::{Read, Write};
use std::process::Command;

use regex::Regex;

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

/// Builds a string containing all lines except those in the excluded range.
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
fn filter_used_imports<'a>(import_names: &[&'a str], rest_of_file: &str) -> Vec<&'a str> {
    import_names.iter().filter(|name| is_import_used(name, rest_of_file)).copied().collect()
}

/// Removes unused imports from Cairo0 source code.
/// Returns the content with unused imports removed.
pub fn remove_unused_cairo0_imports(content: &str) -> String {
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
fn is_import_used(import_item: &str, code: &str) -> bool {
    let name_to_search = get_imported_name(import_item);
    // Match identifier as a whole word (not part of another identifier)
    let pattern = format!(r"\b{}\b", regex::escape(name_to_search));
    let re = Regex::new(&pattern).unwrap();
    re.is_match(code)
}

/// Runs the Cairo0 formatter on the input source code.
pub fn cairo0_format(unformatted: &String) -> String {
    let script_type = Cairo0Script::Format;
    verify_cairo0_compiler_deps(&script_type);

    // Remove unused imports first (before formatting, since removal might break formatting).
    let without_unused_imports = remove_unused_cairo0_imports(unformatted);

    // Dump string to temporary file.
    let mut temp_file = tempfile::NamedTempFile::new().unwrap();
    temp_file.write_all(without_unused_imports.as_bytes()).unwrap();

    // Run formatter.
    let mut command = Command::new(script_type.script_name());
    command.arg(temp_file.path().to_str().unwrap());
    let format_output = command.output().unwrap();
    let stderr_output = String::from_utf8(format_output.stderr).unwrap();
    assert!(format_output.status.success(), "{stderr_output}");

    // Run isort on the formatted file.
    // Note: cairo-format writes to stdout, so we write that to a temp file for isort.
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

    // Read and return the final result.
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
