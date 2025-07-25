use std::path::PathBuf;

use apollo_compilation_utils::compiler_utils::compile_with_args;
use apollo_compilation_utils::errors::CompilationUtilError;
use apollo_compilation_utils::resource_limits::ResourceLimits;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass;
use tracing::info;

use crate::config::SierraCompilationConfig;
use crate::constants::CAIRO_LANG_BINARY_NAME;

#[derive(Clone)]
pub struct SierraToCasmCompiler {
    pub config: SierraCompilationConfig,
    path_to_binary: PathBuf,
}

impl SierraToCasmCompiler {
    pub fn new(config: SierraCompilationConfig) -> Self {
        let path_to_binary = CAIRO_LANG_BINARY_NAME.into();
        info!("Using Sierra compiler binary: {:?}", path_to_binary);
        Self { config, path_to_binary }
    }

    pub fn compile(
        &self,
        contract_class: ContractClass,
    ) -> Result<CasmContractClass, CompilationUtilError> {
        let compiler_binary_path = &self.path_to_binary;
        let additional_args = &[
            "--add-pythonic-hints",
            "--max-bytecode-size",
            &self.config.max_bytecode_size.to_string(),
            // TODO(Shahak, Elin): Fix this in a safe way.
            "--allowed-libfuncs-list-name",
            "audited",
        ];
        let resource_limits = ResourceLimits::new(None, None, self.config.max_memory_usage);

        let stdout = compile_with_args(
            compiler_binary_path,
            contract_class,
            additional_args,
            resource_limits,
        )?;
        Ok(serde_json::from_slice::<CasmContractClass>(&stdout)?)
    }
}
