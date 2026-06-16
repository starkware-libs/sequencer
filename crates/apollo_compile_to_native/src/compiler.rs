use std::path::PathBuf;
#[cfg(feature = "with-libfunc-profiling")]
use std::sync::Arc;

use apollo_compilation_utils::build_utils::verify_compiler_binary;
use apollo_compilation_utils::compiler_utils::compile_with_args;
use apollo_compilation_utils::errors::CompilationUtilError;
use apollo_compilation_utils::paths::binary_path;
use apollo_compilation_utils::resource_limits::ResourceLimits;
use apollo_compile_to_native_types::SierraCompilationConfig;
use cairo_lang_starknet_classes::contract_class::ContractClass;
use cairo_native::executor::AotContractExecutor;
#[cfg(feature = "with-libfunc-profiling")]
use cairo_native::executor::AotWithProgram;
use tempfile::NamedTempFile;

use crate::constants::{CAIRO_NATIVE_BINARY_NAME, REQUIRED_CAIRO_NATIVE_VERSION};

#[derive(Clone)]
pub struct SierraToNativeCompiler {
    pub config: SierraCompilationConfig,
    path_to_binary: PathBuf,
}

impl SierraToNativeCompiler {
    pub fn new(config: SierraCompilationConfig) -> Self {
        let path_to_binary = match &config.compiler_binary_path {
            Some(path) => path.clone(),
            None => {
                let path = binary_path(CAIRO_NATIVE_BINARY_NAME, REQUIRED_CAIRO_NATIVE_VERSION);
                verify_compiler_binary(&path, REQUIRED_CAIRO_NATIVE_VERSION);
                path
            }
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
            Some(self.config.max_cpu_time),
            self.config.max_file_size,
            Some(self.config.max_memory_usage),
        );
        let _stdout = compile_with_args(
            compiler_binary_path,
            contract_class,
            &additional_args,
            resource_limits,
        )?;

        Ok(AotContractExecutor::from_path(output_file.path())
            .map_err(|e| CompilationUtilError::CompilationError(e.to_string()))?
            .unwrap())
    }

    /// Like [`Self::compile`], but also returns the Sierra program so cairo-native's
    /// libfunc profiler can resolve runtime libfunc IDs back to declarations. The
    /// program is extracted from `contract_class` before compilation so the two are
    /// guaranteed to correspond.
    #[cfg(feature = "with-libfunc-profiling")]
    pub fn compile_with_program(
        &self,
        contract_class: ContractClass,
    ) -> Result<AotWithProgram, CompilationUtilError> {
        let program = contract_class
            .extract_sierra_program(false)
            .map(|extracted| Arc::new(extracted.program))
            .map_err(|err| {
                CompilationUtilError::UnexpectedError(format!(
                    "Failed to extract Sierra program for profiling: {err}"
                ))
            })?;
        let executor = self.compile(contract_class)?;
        Ok(AotWithProgram { executor, program })
    }
}
