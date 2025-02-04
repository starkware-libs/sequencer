//! A lib for compiling Sierra into Casm.
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass as CairoLangContractClass;
#[cfg(feature = "cairo_native")]
use cairo_native::executor::AotContractExecutor;
use config::SierraCompilationConfig;
use starknet_api::contract_class::{ContractClass, SierraVersion};
use starknet_api::core::CompiledClassHash;
use starknet_api::state::SierraContractClass;
use starknet_api::StarknetApiError;
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_sierra_multicompile_types::{RawClass, RawExecutableClass, RawExecutableHashedClass};
use thiserror::Error;

use crate::command_line_compiler::CommandLineCompiler;
use crate::errors::CompilationUtilError;
use crate::utils::into_contract_class_for_compilation;

pub mod command_line_compiler;
pub mod communication;
pub mod config;
pub mod constants;
pub mod errors;
pub mod paths;
pub mod resource_limits;
pub mod utils;

#[cfg(test)]
pub mod test_utils;

#[cfg(test)]
#[path = "compile_test.rs"]
pub mod compile_test;

pub type SierraCompilerResult<T> = Result<T, SierraCompilerError>;

pub trait SierraToCasmCompiler: Send + Sync {
    fn compile(
        &self,
        contract_class: CairoLangContractClass,
    ) -> Result<CasmContractClass, CompilationUtilError>;
}

#[cfg(feature = "cairo_native")]
pub trait SierraToNativeCompiler: Send + Sync {
    fn compile_to_native(
        &self,
        contract_class: CairoLangContractClass,
    ) -> Result<AotContractExecutor, CompilationUtilError>;
}

#[derive(Debug, Error)]
pub enum SierraCompilerError {
    #[error(transparent)]
    ClassSerde(#[from] serde_json::Error),
    #[error(transparent)]
    CompilationFailed(#[from] CompilationUtilError),
    #[error("Failed to parse Sierra version: {0}")]
    SierraVersionFormat(StarknetApiError),
}

impl From<SierraCompilerError> for starknet_sierra_multicompile_types::SierraCompilerError {
    fn from(error: SierraCompilerError) -> Self {
        starknet_sierra_multicompile_types::SierraCompilerError::CompilationFailed(
            error.to_string(),
        )
    }
}

// TODO(Elin): consider generalizing the compiler if invocation implementations are added.
#[derive(Clone)]
pub struct SierraCompiler {
    compiler: CommandLineCompiler,
}

impl SierraCompiler {
    pub fn new(compiler: CommandLineCompiler) -> Self {
        Self { compiler }
    }

    // TODO(Elin): move (de)serialization to infra. layer.
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
    // TODO(Elin): rewrite this function
    let compiler = CommandLineCompiler::new(config);
    SierraCompiler::new(compiler)
}

impl ComponentStarter for SierraCompiler {}
