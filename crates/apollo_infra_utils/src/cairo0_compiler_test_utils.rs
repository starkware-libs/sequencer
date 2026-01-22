use std::collections::HashMap;
use std::path::PathBuf;
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
/// For processing multiple files, use `cairo0_format_batch` for better performance.
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
pub fn cairo0_format_batch<S: AsRef<str>>(files: HashMap<String, S>) -> HashMap<String, String> {
    if files.is_empty() {
        return HashMap::new();
    }

    let script_type = Cairo0Script::Format;
    verify_cairo0_compiler_deps(&script_type);

    // Create a temporary directory and write all files to it.
    let temp_dir = tempfile::TempDir::new().unwrap();
    let mut file_paths: Vec<PathBuf> = Vec::with_capacity(files.len());
    let mut filenames: Vec<String> = Vec::with_capacity(files.len());

    // First stage: remove unused imports before writing to temp files.
    for (filename, content) in files {
        let without_unused = remove_unused_cairo0_imports(content.as_ref());
        let file_path = temp_dir.path().join(&filename);
        // Create parent directories if needed.
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&file_path, without_unused).unwrap();
        file_paths.push(file_path);
        filenames.push(filename);
    }

    // Run cairo-format on all files at once with -i (in-place).
    let mut format_command = Command::new(script_type.script_name());
    format_command.arg("-i");
    for path in &file_paths {
        format_command.arg(path);
    }
    run_command(format_command);

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
    run_command(isort_command);

    // Read back all formatted files.
    filenames
        .into_iter()
        .zip(file_paths.iter().map(|path| std::fs::read_to_string(path).unwrap()))
        .collect()
}

fn run_command(mut cmd: Command) {
    let output = cmd.output().unwrap();
    let stderr_output = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "Command '{cmd:?}' failed: {stderr_output}");
}
