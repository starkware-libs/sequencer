use std::fs;
use log;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass;
#[cfg(feature = "cairo_native")]
use cairo_native::executor::AotContractExecutor;
use tempfile::NamedTempFile;

use crate::config::SierraCompilationConfig;
use crate::constants::CAIRO_LANG_BINARY_NAME;
#[cfg(feature = "cairo_native")]
use crate::constants::CAIRO_NATIVE_BINARY_NAME;
use crate::errors::CompilationUtilError;
use crate::paths::binary_path;
use crate::resource_limits::ResourceLimits;
use crate::SierraToCasmCompiler;
#[cfg(feature = "cairo_native")]
use crate::SierraToNativeCompiler;

#[derive(Clone)]
pub struct CommandLineCompiler {
    pub config: SierraCompilationConfig,
    path_to_starknet_sierra_compile_binary: PathBuf,
    #[cfg(feature = "cairo_native")]
    path_to_starknet_native_compile_binary: PathBuf,
}

impl CommandLineCompiler {
    pub fn new(config: SierraCompilationConfig) -> Self {
        let path_to_starknet_sierra_compile_binary = binary_path(out_dir(), CAIRO_LANG_BINARY_NAME);
        #[cfg(feature = "cairo_native")]
        let path_to_starknet_native_compile_binary = match &config.sierra_to_native_compiler_path {
            Some(path) => path.clone(),
            None => binary_path(out_dir(), CAIRO_NATIVE_BINARY_NAME),
        };
        Self {
            config,
            path_to_starknet_sierra_compile_binary,
            #[cfg(feature = "cairo_native")]
            path_to_starknet_native_compile_binary,
        }
    }
}

impl SierraToCasmCompiler for CommandLineCompiler {
    fn compile(
        &self,
        contract_class: ContractClass,
    ) -> Result<CasmContractClass, CompilationUtilError> {
        let compiler_binary_path = &self.path_to_starknet_sierra_compile_binary;
        let additional_args = &[
            "--add-pythonic-hints",
            "--max-bytecode-size",
            &self.config.max_casm_bytecode_size.to_string(),
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

#[cfg(feature = "cairo_native")]
impl SierraToNativeCompiler for CommandLineCompiler {
    fn compile_to_native(
        &self,
        contract_class: ContractClass,
    ) -> Result<AotContractExecutor, CompilationUtilError> {
        let compiler_binary_path = &self.path_to_starknet_native_compile_binary;

        let output_file = NamedTempFile::new()?;
        let output_file_path = output_file.path().to_str().ok_or(
            CompilationUtilError::UnexpectedError("Failed to get output file path".to_owned()),
        )?;
        let optimization_level = self.config.optimization_level.to_string();
        let additional_args = [output_file_path, "--opt-level", &optimization_level];
        let resource_limits = ResourceLimits::new(
            Some(self.config.max_cpu_time),
            Some(self.config.max_native_bytecode_size),
            Some(self.config.max_memory_usage),
        );
        let _stdout = compile_with_args(
            compiler_binary_path,
            contract_class,
            &additional_args,
            resource_limits,
        )?;

        let file_size_bytes = fs::metadata(Path::new(&output_file_path))?.len();
        let file_size_mb = file_size_bytes as f64 / (1024.0 * 1024.0);
        log::debug!("Compiled native file size: {:.2} MB", file_size_mb);
        Ok(AotContractExecutor::from_path(Path::new(&output_file_path))?.unwrap())
    }

    fn panic_on_compilation_failure(&self) -> bool {
        self.config.panic_on_compilation_failure
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
