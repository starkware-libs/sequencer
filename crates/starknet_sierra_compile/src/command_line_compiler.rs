#[cfg(feature = "cairo_native")]
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass;
#[cfg(feature = "cairo_native")]
use cairo_native::executor::AotContractExecutor;

use crate::build_utils::{binary_path, CAIRO_LANG_BINARY_NAME};
#[cfg(feature = "cairo_native")]
use crate::build_utils::{output_file_path, CAIRO_NATIVE_BINARY_NAME};
use crate::config::SierraToCasmCompilationConfig;
use crate::errors::CompilationUtilError;
use crate::utils::{process_compile_command_output, save_contract_class_to_temp_file};
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
        let temp_file = save_contract_class_to_temp_file(contract_class)?;
        let temp_file_path = temp_file.path().to_str().ok_or(
            CompilationUtilError::UnexpectedError("Failed to get temporary file path".to_owned()),
        )?;

        // Set the parameters for the compile process.
        // TODO(Arni): Setup the ulimit for the process.
        let mut command = Command::new(self.path_to_starknet_sierra_compile_binary.as_os_str());
        command.args([
            temp_file_path,
            "--add-pythonic-hints",
            "--max-bytecode-size",
            &self.config.max_bytecode_size.to_string(),
        ]);

        let stdout = process_compile_command_output(command.output()?)?;
        Ok(serde_json::from_slice::<CasmContractClass>(&stdout)?)
    }
}

#[cfg(feature = "cairo_native")]
impl SierraToNativeCompiler for CommandLineCompiler {
    fn compile_to_native(
        &self,
        contract_class: ContractClass,
    ) -> Result<AotContractExecutor, CompilationUtilError> {
        let temp_file = save_contract_class_to_temp_file(contract_class)?;
        let temp_file_path = temp_file.path().to_str().ok_or(
            CompilationUtilError::UnexpectedError("Failed to get temporary file path".to_owned()),
        )?;

        let output_file_path = output_file_path();

        // Set the parameters for the compile process.
        // TODO(Avi, 01/12/2024): Limit the process memory, time and output size.
        let mut command = Command::new(self.path_to_starknet_native_compile_binary.as_os_str());
        command.args([temp_file_path, &output_file_path]);

        process_compile_command_output(command.output()?)?;

        Ok(AotContractExecutor::load(Path::new(&output_file_path))?)
    }
}
