use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass;
use tempfile::NamedTempFile;

use crate::config::SierraCompilationConfig;
use crate::constants::CAIRO_LANG_BINARY_NAME;
use crate::errors::CompilationUtilError;
use crate::paths::binary_path;
use crate::resource_limits::ResourceLimits;
use crate::SierraToCasmCompiler;

#[derive(Clone)]
pub struct CommandLineCompiler {
    pub config: SierraCompilationConfig,
    path_to_binary: PathBuf,
}

impl CommandLineCompiler {
    pub fn new(config: SierraCompilationConfig) -> Self {
        Self { config, path_to_binary: binary_path(out_dir(), CAIRO_LANG_BINARY_NAME) }
    }
}

impl SierraToCasmCompiler for CommandLineCompiler {
    fn compile(
        &self,
        contract_class: ContractClass,
    ) -> Result<CasmContractClass, CompilationUtilError> {
        let compiler_binary_path = &self.path_to_binary;
        let additional_args = &[
            "--add-pythonic-hints",
            "--max-bytecode-size",
            &self.config.max_file_size.to_string(),
        ];
        let resource_limits = ResourceLimits::new(None, None, None);

        let stdout = compile_with_args(
            compiler_binary_path,
            contract_class,
            additional_args,
            resource_limits,
        )?;
        Ok(serde_json::from_slice::<CasmContractClass>(&stdout)?)
    }
}

fn compile_with_args(
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
    // TODO(Arni, Avi): Setup the ulimit for the process.
    let mut command = Command::new(compiler_binary_path.as_os_str());
    command.arg(temp_file_path).args(additional_args);

    // Apply the resource limits to the command.
    resource_limits.apply(&mut command);

    // Run the compile process.
    let compile_output = command.output()?;

    if !compile_output.status.success() {
        let stderr_output = String::from_utf8(compile_output.stderr)
            .unwrap_or("Failed to get stderr output".into());
        // TODO(Avi, 28/2/2025): Make the error messages more readable.
        return Err(CompilationUtilError::CompilationError(format!(
            "Exit status: {}\n Stderr: {}",
            compile_output.status, stderr_output
        )));
    };
    Ok(compile_output.stdout)
}

// Returns the OUT_DIR. This function is only operable at run time.
fn out_dir() -> PathBuf {
    env!("RUNTIME_ACCESSIBLE_OUT_DIR").into()
}
