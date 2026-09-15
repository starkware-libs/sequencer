use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use apollo_compilation_utils::build_utils::verify_compiler_binary;
use apollo_compilation_utils::compiler_utils::compile_with_args;
use apollo_compilation_utils::errors::CompilationUtilError;
use apollo_compilation_utils::libfunc_arg::LibfuncArg;
use apollo_compilation_utils::paths::binary_path;
use apollo_compilation_utils::resource_limits::ResourceLimits;
use apollo_infra_utils::cairo_compiler_version::CAIRO1_COMPILER_VERSION;
use apollo_sierra_compilation_config::config::{AllowedLibfuncsList, SierraCompilationConfig};
use cairo_lang_starknet_classes::allowed_libfuncs::AllowedLibfuncs;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass;
use tempfile::NamedTempFile;
use tracing::info;

use crate::constants::CAIRO_LANG_BINARY_NAME;

/// The allowed-libfuncs list this binary ships with, selected by [`AllowedLibfuncsList::Bundled`].
pub(crate) const BUNDLED_ALLOWED_LIBFUNCS: &str = include_str!("allowed_libfuncs.json");

#[derive(Clone)]
pub struct SierraToCasmCompiler {
    pub config: SierraCompilationConfig,
    path_to_binary: PathBuf,
    /// The libfunc list flag and value, resolved once so that a misconfigured list fails at
    /// startup rather than on every compilation.
    libfunc_args: [String; 2],
    /// Keeps the materialized [`AllowedLibfuncsList::Bundled`] file alive for as long as any clone
    /// of this compiler can run.
    _bundled_libfuncs_file: Option<Arc<NamedTempFile>>,
}

impl SierraToCasmCompiler {
    pub fn new(config: SierraCompilationConfig) -> Self {
        let path_to_binary = binary_path(CAIRO_LANG_BINARY_NAME, CAIRO1_COMPILER_VERSION);
        verify_compiler_binary(&path_to_binary, CAIRO1_COMPILER_VERSION);
        info!("Using Sierra compiler binary at: {:?}", path_to_binary);

        let bundled_libfuncs_file = match config.allowed_libfuncs_list {
            AllowedLibfuncsList::Bundled => Some(Arc::new(write_bundled_allowed_libfuncs())),
            _ => None,
        };
        let libfunc_arg = resolve_libfunc_arg(
            &config.allowed_libfuncs_list,
            bundled_libfuncs_file.as_ref().map(|file| file.path()),
        );
        info!("Compiling against libfunc list: {}", config.allowed_libfuncs_list);
        let libfunc_args = libfunc_arg.to_cli_args().unwrap_or_else(|| {
            panic!("Allowed libfuncs list path {libfunc_arg:?} is not valid UTF-8.")
        });

        Self { config, path_to_binary, libfunc_args, _bundled_libfuncs_file: bundled_libfuncs_file }
    }

    pub fn compile(
        &self,
        contract_class: ContractClass,
    ) -> Result<CasmContractClass, CompilationUtilError> {
        let compiler_binary_path = &self.path_to_binary;
        let [libfunc_flag, libfunc_value] = &self.libfunc_args;
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

/// Panics on a list file the compiler would reject anyway, so the failure surfaces at startup.
fn resolve_libfunc_arg(
    allowed_libfuncs_list: &AllowedLibfuncsList,
    bundled_libfuncs_path: Option<&Path>,
) -> LibfuncArg {
    match allowed_libfuncs_list {
        AllowedLibfuncsList::Audited => LibfuncArg::ListName("audited".to_string()),
        AllowedLibfuncsList::All => LibfuncArg::ListName("all".to_string()),
        AllowedLibfuncsList::Bundled => LibfuncArg::ListFile(
            bundled_libfuncs_path.expect("Bundled list file must be materialized.").to_path_buf(),
        ),
        AllowedLibfuncsList::File(path) => {
            let contents = std::fs::read_to_string(path).unwrap_or_else(|error| {
                panic!("Failed to read allowed libfuncs list {}: {error}", path.display())
            });
            serde_json::from_str::<AllowedLibfuncs>(&contents).unwrap_or_else(|error| {
                panic!("Failed to parse allowed libfuncs list {}: {error}", path.display())
            });
            LibfuncArg::ListFile(path.clone())
        }
    }
}

/// The compiler binary only reads the list from disk, so the bundled list is written to a
/// temporary file rather than shipped alongside the node.
fn write_bundled_allowed_libfuncs() -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("Failed to create the bundled libfuncs list file.");
    file.write_all(BUNDLED_ALLOWED_LIBFUNCS.as_bytes())
        .expect("Failed to write the bundled libfuncs list file.");
    file.flush().expect("Failed to flush the bundled libfuncs list file.");
    file
}
