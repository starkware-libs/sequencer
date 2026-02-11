use std::path::PathBuf;
use std::sync::Once;

use apollo_compilation_utils::build_utils::install_compiler_binary;
use apollo_compilation_utils::compiler_utils::compile_with_args;
use apollo_compilation_utils::errors::CompilationUtilError;
use apollo_compilation_utils::paths::binary_path;
use apollo_compilation_utils::resource_limits::ResourceLimits;
use apollo_infra_utils::cairo_compiler_version::CAIRO1_COMPILER_VERSION;
use apollo_sierra_compilation_config::config::SierraCompilationConfig;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass;
use tracing::info;

use crate::constants::CAIRO_LANG_BINARY_NAME;

static STARKNET_SIERRA_COMPILE_INSTALLER: Once = Once::new();

#[derive(Clone)]
pub struct SierraToCasmCompiler {
    pub config: SierraCompilationConfig,
    path_to_binary: PathBuf,
}

impl SierraToCasmCompiler {
    pub fn new(config: SierraCompilationConfig) -> Self {
        let runtime_out_dir = out_dir();
        ensure_starknet_sierra_compile_installed(&runtime_out_dir);
        let path_to_binary = binary_path(&runtime_out_dir, CAIRO_LANG_BINARY_NAME);
        info!("Using Sierra compiler binary at: {:?}", path_to_binary);
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
            "--allowed-libfuncs-list-name",
            if self.config.audited_libfuncs_only { "audited" } else { "all" },
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

fn ensure_starknet_sierra_compile_installed(out_dir: &std::path::Path) {
    STARKNET_SIERRA_COMPILE_INSTALLER.call_once(|| {
        let cargo_install_args = [CAIRO_LANG_BINARY_NAME, "--version", CAIRO1_COMPILER_VERSION];
        install_compiler_binary(
            CAIRO_LANG_BINARY_NAME,
            CAIRO1_COMPILER_VERSION,
            &cargo_install_args,
            out_dir,
        );
    });
}

// Returns the OUT_DIR. This function is only operable at run time.
fn out_dir() -> PathBuf {
    env!("RUNTIME_ACCESSIBLE_OUT_DIR").into()
}
