use std::io::Write;
// TODO(Avi, 01/06/2025): Adapt this import to make the crate compile on windows.
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::Command;

use cairo_lang_starknet_classes::contract_class::ContractClass;
use tempfile::NamedTempFile;

use crate::errors::CompilationUtilError;
use crate::resource_limits::ResourceLimits;

#[cfg(test)]
#[path = "compiler_utils_test.rs"]
pub mod test;

pub fn compile_with_args(
    compiler_binary_path: &Path,
    contract_class: ContractClass,
    additional_args: &[&str],
    resource_limits: ResourceLimits,
) -> Result<Vec<u8>, CompilationUtilError> {
    // Create a temporary file to store the Sierra contract class.
    let serialized_contract_class = serde_json::to_string(&contract_class)?;

    let mut temp_file = NamedTempFile::new()?;
    temp_file.write_all(serialized_contract_class.as_bytes())?;
    let temp_file_path = temp_file.path().to_str().ok_or(CompilationUtilError::UnexpectedError(
        "Failed to get temporary file path".to_owned(),
    ))?;

    // Set the parameters for the compile process.
    let mut command = Command::new(compiler_binary_path.as_os_str());
    command.arg(temp_file_path).args(additional_args);

    // Apply the resource limits to the command.
    resource_limits.apply(&mut command);

    // Run the compile process.
    let compile_output = command.output()?;

    if !compile_output.status.success() {
        let stderr_output = String::from_utf8(compile_output.stderr)
            .unwrap_or_else(|_| "Failed to decode stderr output".to_string());

        let error_message = format_compiler_error(&stderr_output, &compile_output.status);
        return Err(CompilationUtilError::CompilationError(error_message));
    }
    Ok(compile_output.stdout)
}

/// Extracts the meaningful error lines from compiler stderr, filtering out stack backtraces and
/// resource limit setup messages.
fn extract_error_from_stderr(stderr: &str) -> String {
    let meaningful_lines: Vec<&str> = stderr
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            // Skip resource limit setup lines (e.g., "Setting Resource::CPU limits: ...").
            if trimmed.starts_with("Setting Resource::") {
                return false;
            }
            // Skip stack backtrace header and frames (e.g., "0: module::path").
            if trimmed.eq_ignore_ascii_case("stack backtrace:")
                || trimmed.starts_with("Signal info:")
            {
                return false;
            }
            if let Some(rest) = trimmed.strip_prefix(|c: char| c.is_ascii_digit()) {
                let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit());
                if rest.starts_with(':') {
                    return false;
                }
            }
            !trimmed.is_empty()
        })
        .collect();

    if meaningful_lines.is_empty() {
        return "Compilation failed with no meaningful error output".to_string();
    }

    meaningful_lines.join("\n")
}

pub(crate) fn format_compiler_error(stderr: &str, status: &std::process::ExitStatus) -> String {
    let error_detail = extract_error_from_stderr(stderr);
    let signal_description = match status.signal() {
        Some(9) => Some(
            "process was killed (SIGKILL), possibly due to exceeding CPU or memory limits"
                .to_string(),
        ),
        Some(25) => Some("file size limit exceeded (SIGXFSZ)".to_string()),
        Some(signal) => Some(format!("process terminated by signal {signal}")),
        None => None,
    };

    match signal_description {
        Some(signal) => format!("{error_detail} ({signal})"),
        None => error_detail,
    }
}
