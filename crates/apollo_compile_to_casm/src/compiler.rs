use std::path::PathBuf;

use apollo_compilation_utils::build_utils::verify_compiler_binary;
use apollo_compilation_utils::compiler_utils::compile_with_args;
use apollo_compilation_utils::errors::CompilationUtilError;
use apollo_compilation_utils::paths::binary_path;
use apollo_compilation_utils::resource_limits::ResourceLimits;
use apollo_infra_utils::cairo_compiler_version::CAIRO1_COMPILER_VERSION;
use apollo_infra_utils::path::resolve_project_relative_path;
use apollo_sierra_compilation_config::config::{AllowedLibfuncsList, SierraCompilationConfig};
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass;
use tracing::info;

use crate::constants::{BUNDLED_ALLOWED_LIBFUNCS_PATH, CAIRO_LANG_BINARY_NAME};

#[cfg(test)]
#[path = "compiler_test.rs"]
mod compiler_test;

#[derive(Clone)]
pub struct SierraToCasmCompiler {
    pub config: SierraCompilationConfig,
    path_to_binary: PathBuf,
    /// The libfunc list flag and value, resolved once so that a missing bundled list fails at
    /// startup rather than on every compilation.
    libfunc_list_arg: (&'static str, String),
}

impl SierraToCasmCompiler {
    pub fn new(config: SierraCompilationConfig) -> Self {
        let path_to_binary = binary_path(CAIRO_LANG_BINARY_NAME, CAIRO1_COMPILER_VERSION);
        verify_compiler_binary(&path_to_binary, CAIRO1_COMPILER_VERSION);
        info!("Using Sierra compiler binary at: {:?}", path_to_binary);
        let libfunc_list_arg = libfunc_list_arg(config.allowed_libfuncs_list);
        Self { config, path_to_binary, libfunc_list_arg }
    }

    pub fn compile(
        &self,
        contract_class: ContractClass,
    ) -> Result<CasmContractClass, CompilationUtilError> {
        let compiler_binary_path = &self.path_to_binary;
        let (libfunc_flag, libfunc_value) = &self.libfunc_list_arg;
        let additional_args = &[
            "--add-pythonic-hints",
            "--max-bytecode-size",
            &self.config.max_bytecode_size.to_string(),
            libfunc_flag,
            libfunc_value,
        ];
        let resource_limits = ResourceLimits::new(
            Some(self.config.max_cpu_time),
            None,
            Some(self.config.max_memory_usage),
        );

        let stdout = compile_with_args(
            compiler_binary_path,
            contract_class,
            additional_args,
            resource_limits,
        )?;
        Ok(serde_json::from_slice::<CasmContractClass>(&stdout)?)
    }
}

/// The compiler flag and value selecting the configured libfunc list.
///
/// Panics if the bundled list is missing, so an image that failed to ship it fails at startup.
pub(crate) fn libfunc_list_arg(
    allowed_libfuncs_list: AllowedLibfuncsList,
) -> (&'static str, String) {
    match allowed_libfuncs_list {
        AllowedLibfuncsList::Audited => ("--allowed-libfuncs-list-name", "audited".to_owned()),
        AllowedLibfuncsList::All => ("--allowed-libfuncs-list-name", "all".to_owned()),
        AllowedLibfuncsList::Bundled => (
            "--allowed-libfuncs-list-file",
            resolve_project_relative_path(BUNDLED_ALLOWED_LIBFUNCS_PATH)
                .unwrap_or_else(|error| {
                    panic!("Failed to resolve {BUNDLED_ALLOWED_LIBFUNCS_PATH}: {error}")
                })
                .to_string_lossy()
                .into_owned(),
        ),
    }
}
