use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass;
#[cfg(feature = "cairo_native")]
use cairo_native::executor::AotContractExecutor;
use tempfile::NamedTempFile;

use crate::build_utils::{binary_path, CAIRO_LANG_BINARY_NAME};
#[cfg(feature = "cairo_native")]
use crate::build_utils::{output_file_path, CAIRO_NATIVE_BINARY_NAME};
use crate::config::SierraToCasmCompilationConfig;
use crate::errors::CompilationUtilError;
use crate::SierraToCasmCompiler;
#[cfg(feature = "cairo_native")]
use crate::SierraToNativeCompiler;

#[derive(Clone)]
pub struct CommandLineCompiler {
    pub config: SierraToCasmCompilationConfig,
    path_to_starknet_sierra_compile_binary: PathBuf,
    #[cfg(feature = "cairo_native")]
    path_to_starknet_native_compile_binary: PathBuf,
}

impl CommandLineCompiler {
    pub fn new(config: SierraToCasmCompilationConfig) -> Self {
        Self {
            config,
            path_to_starknet_sierra_compile_binary: binary_path(CAIRO_LANG_BINARY_NAME),
            #[cfg(feature = "cairo_native")]
            path_to_starknet_native_compile_binary: binary_path(CAIRO_NATIVE_BINARY_NAME),
        }
    }
}

impl SierraToCasmCompiler for CommandLineCompiler {
    fn compile(
        &self,
        contract_class: ContractClass,
    ) -> Result<CasmContractClass, CompilationUtilError> {
        let compiler_binary_path = &self.path_to_starknet_sierra_compile_binary;
        let additional_args = [
            "--add-pythonic-hints",
            "--max-bytecode-size",
            &self.config.max_bytecode_size.to_string(),
        ];

        let stdout = compile_with_args(compiler_binary_path, contract_class, &additional_args)?;
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
        let output_file_path = output_file_path();
        let additional_args = [output_file_path.as_str()];

        let _stdout = compile_with_args(compiler_binary_path, contract_class, &additional_args)?;

        Ok(AotContractExecutor::load(Path::new(&output_file_path))?)
    }
}

fn compile_with_args(
    compiler_binary_path: &Path,
    contract_class: ContractClass,
    additional_args: &[&str],
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

    // Run the compile process.
    let compile_output = command.output()?;

    if !compile_output.status.success() {
        let stderr_output = String::from_utf8(compile_output.stderr)
            .unwrap_or("Failed to get stderr output".into());
        return Err(CompilationUtilError::CompilationError(stderr_output));
    };
    Ok(compile_output.stdout)
}
