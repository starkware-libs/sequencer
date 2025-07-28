use std::path::{Path, PathBuf};

use apollo_compilation_utils::compiler_utils::compile_with_args;
use apollo_compilation_utils::errors::CompilationUtilError;
use apollo_compilation_utils::paths::binary_path;
use apollo_compilation_utils::resource_limits::ResourceLimits;
use apollo_compile_to_native_types::SierraCompilationConfig;
use cairo_lang_starknet_classes::contract_class::ContractClass;
use cairo_native::executor::AotContractExecutor;
use tempfile::NamedTempFile;

use crate::constants::CAIRO_NATIVE_BINARY_NAME;

#[derive(Clone)]
pub struct SierraToNativeCompiler {
    pub config: SierraCompilationConfig,
    path_to_binary: PathBuf,
}

impl SierraToNativeCompiler {
    pub fn new(config: SierraCompilationConfig) -> Self {
        let path_to_binary = match &config.compiler_binary_path {
            Some(path) => path.clone(),
            None => binary_path(&out_dir(), CAIRO_NATIVE_BINARY_NAME),
        };
        Self { config, path_to_binary }
    }

    pub fn compile(
        &self,
        contract_class: ContractClass,
    ) -> Result<AotContractExecutor, CompilationUtilError> {
        let compiler_binary_path = &self.path_to_binary;

        let output_file = NamedTempFile::new()?;
        let output_file_path = output_file.path().to_str().ok_or(
            CompilationUtilError::UnexpectedError("Failed to get output file path".to_owned()),
        )?;
        let optimization_level = self.config.optimization_level.to_string();
        let additional_args = [output_file_path, "--opt-level", &optimization_level];
        let resource_limits = ResourceLimits::new(
            self.config.max_cpu_time,
            self.config.max_file_size,
            self.config.max_memory_usage,
        );
        let _stdout = compile_with_args(
            compiler_binary_path,
            contract_class,
            &additional_args,
            resource_limits,
        )?;

        Ok(AotContractExecutor::from_path(Path::new(&output_file_path))
            .map_err(|e| CompilationUtilError::CompilationError(e.to_string()))?
            .unwrap())
    }
}

// Returns the OUT_DIR. This function is only operable at run time.
fn out_dir() -> PathBuf {
    env!("RUNTIME_ACCESSIBLE_OUT_DIR").into()
}
