use std::io::{Read, Write};
use std::process::Command;

use regex::Regex;

use crate::cairo0_compiler::{
    cairo0_script_correct_version,
    enter_venv_instructions,
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

/// Checks if a line is an import statement.
fn is_import_line(line: &str) -> bool {
    (line.starts_with("from ") && line.contains(" import ")) || line.starts_with("import ")
}

/// Finds the line index where the import section ends.
fn find_imports_end(lines: &[&str]) -> usize {
    let mut in_multi_line_import = false;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if in_multi_line_import {
            if trimmed.starts_with(')') {
                in_multi_line_import = false;
            }
        } else if is_import_line(trimmed) {
            in_multi_line_import = trimmed.ends_with('(');
        } else if !trimmed.is_empty() {
            return i;
        }
    }
    lines.len()
}

/// Removes unused imports from Cairo0 source code.
/// Returns the content with unused imports removed (formatting will be fixed by cairo-format).
pub fn remove_unused_cairo0_imports(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut result_lines: Vec<String> = Vec::new();
    let mut i = 0;

    // Find where imports end and build the code section once
    let imports_end = find_imports_end(&lines);
    let code_section = lines[imports_end..].join("\n");

    // Regex to match import lines
    let single_import_re = Regex::new(r"^from\s+\S+\s+import\s+(.+)$").unwrap();
    let multi_import_start_re = Regex::new(r"^from\s+\S+\s+import\s+\($").unwrap();
    let direct_import_re = Regex::new(r"^import\s+(.+)$").unwrap();

    while i < lines.len() {
        let line = lines[i];

        // Check if this is a multi-line import (starts with `from X import (`)
        if multi_import_start_re.is_match(line) {
            let mut import_lines: Vec<(usize, &str)> = Vec::new(); // (line_idx, import_name)
            i += 1;

            // Collect all imported names until we hit the closing `)`
            while i < lines.len() && !lines[i].trim().starts_with(')') {
                let import_name = lines[i].trim().trim_end_matches(',').trim();
                if !import_name.is_empty() {
                    import_lines.push((i, import_name));
                }
                i += 1;
            }
            let end_line = i;

            // Keep only used imports
            let used_lines: Vec<_> = import_lines
                .iter()
                .filter(|(_, name)| is_import_used(name, &code_section))
                .collect();

            if !used_lines.is_empty() {
                result_lines.push(line.to_string()); // `from X import (`
                for (idx, _) in &used_lines {
                    result_lines.push(lines[*idx].to_string());
                }
                result_lines.push(lines[end_line].to_string()); // `)`
            }
            i = end_line + 1;
            continue;
        }

        // Check if this is a single-line `from X import Y` style import
        if let Some(caps) = single_import_re.captures(line) {
            let imports_part = caps.get(1).unwrap().as_str();
            let import_names: Vec<&str> =
                imports_part.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();

            let used_imports: Vec<_> = import_names
                .into_iter()
                .filter(|name| is_import_used(name, &code_section))
                .collect();

            if !used_imports.is_empty() {
                let from_part = line.split(" import ").next().unwrap();
                result_lines.push(format!("{} import {}", from_part, used_imports.join(", ")));
            }
            i += 1;
            continue;
        }

        // Check if this is a direct `import X as Y` style import
        if let Some(caps) = direct_import_re.captures(line) {
            let import_part = caps.get(1).unwrap().as_str().trim();
            if is_import_used(import_part, &code_section) {
                result_lines.push(line.to_string());
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
/// Handles `X as Y` syntax by returning Y.
/// For `import a.b.c`, returns `c` (the last component).
fn get_imported_name(import_item: &str) -> &str {
    let trimmed = import_item.trim();
    // Handle "X as Y" - we need to search for Y in the code
    if let Some(as_pos) = trimmed.find(" as ") {
        trimmed[as_pos + 4..].trim()
    } else if let Some(dot_pos) = trimmed.rfind('.') {
        // For `import a.b.c`, the name used in code is `c`
        &trimmed[dot_pos + 1..]
    } else {
        trimmed
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
pub fn cairo0_format(unformatted: &str) -> String {
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
