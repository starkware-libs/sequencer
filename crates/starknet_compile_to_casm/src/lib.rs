//! A lib for compiling Sierra into Casm.
use apollo_infra::component_definitions::ComponentStarter;
use apollo_sierra_multicompile_types::{RawClass, RawExecutableClass, RawExecutableHashedClass};
use starknet_api::contract_class::{ContractClass, SierraVersion};
use starknet_api::core::CompiledClassHash;
use starknet_api::state::SierraContractClass;
use starknet_api::StarknetApiError;
use starknet_compilation_utils::class_utils::into_contract_class_for_compilation;
use starknet_compilation_utils::errors::CompilationUtilError;
use thiserror::Error;
use tracing::instrument;

use crate::compiler::SierraToCasmCompiler;
use crate::config::SierraCompilationConfig;

pub mod communication;
pub mod compiler;
pub mod config;
pub mod constants;

#[cfg(test)]
#[path = "compile_test.rs"]
pub mod compile_test;

pub type SierraCompilerResult<T> = Result<T, SierraCompilerError>;

#[cfg(test)]
#[path = "constants_test.rs"]
pub mod constants_test;

#[derive(Debug, Error)]
pub enum SierraCompilerError {
    #[error(transparent)]
    ClassSerde(#[from] serde_json::Error),
    #[error(transparent)]
    CompilationFailed(#[from] CompilationUtilError),
    #[error("Failed to parse Sierra version: {0}")]
    SierraVersionFormat(StarknetApiError),
}

impl From<SierraCompilerError> for apollo_sierra_multicompile_types::SierraCompilerError {
    fn from(error: SierraCompilerError) -> Self {
        apollo_sierra_multicompile_types::SierraCompilerError::CompilationFailed(error.to_string())
    }
}

// TODO(Elin): consider generalizing the compiler if invocation implementations are added.
#[derive(Clone)]
pub struct SierraCompiler {
    compiler: SierraToCasmCompiler,
}

impl SierraCompiler {
    pub fn new(compiler: SierraToCasmCompiler) -> Self {
        Self { compiler }
    }

    // TODO(Elin): move (de)serialization to infra. layer.
    #[instrument(skip(self, class), err)]
    pub fn compile(&self, class: RawClass) -> SierraCompilerResult<RawExecutableHashedClass> {
        let class = SierraContractClass::try_from(class)?;
        let sierra_version = SierraVersion::extract_from_program(&class.sierra_program)
            .map_err(SierraCompilerError::SierraVersionFormat)?;
        let class = into_contract_class_for_compilation(&class);

        // TODO(Elin): handle resources (whether here or an infra. layer load-balancing).
        let executable_class = self.compiler.compile(class)?;
        // TODO(Elin): consider spawning a worker for hash calculation.
        let executable_class_hash = CompiledClassHash(executable_class.compiled_class_hash());
        let executable_class = ContractClass::V1((executable_class, sierra_version));
        let executable_class = RawExecutableClass::try_from(executable_class)?;

        Ok((executable_class, executable_class_hash))
    }
}

pub fn create_sierra_compiler(config: SierraCompilationConfig) -> SierraCompiler {
    let compiler = SierraToCasmCompiler::new(config);
    SierraCompiler::new(compiler)
}

impl ComponentStarter for SierraCompiler {}
