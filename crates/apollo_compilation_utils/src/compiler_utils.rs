use std::io::Write;
// TODO(Avi, 01/06/2025): Adapt this import to make the crate compile on windows.
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::Command;

use cairo_lang_starknet_classes::contract_class::ContractClass;
use tempfile::NamedTempFile;

use crate::errors::CompilationUtilError;
use crate::resource_limits::ResourceLimits;

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
        let signal_info = match compile_output.status.signal() {
            Some(9) => {
                "SIGKILL (9): Process was forcefully killed (for example, because it exceeded CPU \
                 limit)."
            }
            Some(25) => "SIGXFSZ (25): File size limit exceeded.",
            None => {
                "Process exited with non-zero status but no signal (likely a handled error, e.g., \
                 memory allocation failure)."
            }
            Some(sig) => &format!("Process terminated by unexpected signal: {sig}"),
        };

        let stderr_output = String::from_utf8(compile_output.stderr)
            .unwrap_or_else(|_| "Failed to decode stderr output".to_string());

        return Err(CompilationUtilError::CompilationError(format!(
            "Exit status: {}\nStderr: {}\nSignal info: {}",
            compile_output.status, stderr_output, signal_info
        )));
    }
    Ok(compile_output.stdout)
}
