use std::path::PathBuf;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass;
use starknet_compilation_utils::compiler_utils::compile_with_args;
use starknet_compilation_utils::errors::CompilationUtilError;
use starknet_compilation_utils::paths::binary_path;
use starknet_compilation_utils::resource_limits::ResourceLimits;

use crate::config::SierraCompilationConfig;
use crate::constants::CAIRO_LANG_BINARY_NAME;
use crate::SierraToCasmCompiler;

#[derive(Clone)]
pub struct CommandLineCompiler {
    pub config: SierraCompilationConfig,
    path_to_binary: PathBuf,
}

impl CommandLineCompiler {
    pub fn new(config: SierraCompilationConfig) -> Self {
        Self { config, path_to_binary: binary_path(&out_dir(), CAIRO_LANG_BINARY_NAME) }
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
            &self.config.max_bytecode_size.to_string(),
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

// Returns the OUT_DIR. This function is only operable at run time.
fn out_dir() -> PathBuf {
    env!("RUNTIME_ACCESSIBLE_OUT_DIR").into()
}
