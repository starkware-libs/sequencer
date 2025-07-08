//! A lib for compiling Sierra into Casm.
use apollo_compilation_utils::class_utils::into_contract_class_for_compilation;
use apollo_compilation_utils::errors::CompilationUtilError;
use apollo_compile_to_casm_types::{RawClass, RawExecutableClass, RawExecutableHashedClass};
use apollo_infra::component_definitions::{default_component_start_fn, ComponentStarter};
use apollo_proc_macros::sequencer_latency_histogram;
use async_trait::async_trait;
use starknet_api::contract_class::{ContractClass, SierraVersion};
use starknet_api::core::CompiledClassHash;
use starknet_api::state::SierraContractClass;
use starknet_api::StarknetApiError;
use thiserror::Error;
use tracing::instrument;

use crate::compiler::SierraToCasmCompiler;
use crate::config::SierraCompilationConfig;
use crate::metrics::{register_metrics, COMPILATION_DURATION};

pub mod communication;
pub mod compiler;
pub mod config;
pub mod constants;
pub mod metrics;

#[cfg(test)]
#[path = "compile_test.rs"]
pub mod compile_test;

pub type SierraCompilerResult<T> = Result<T, SierraCompilerError>;

#[derive(Debug, Error)]
pub enum SierraCompilerError {
    #[error(transparent)]
    ClassSerde(#[from] serde_json::Error),
    #[error(transparent)]
    CompilationFailed(#[from] CompilationUtilError),
    #[error("Failed to parse Sierra version: {0}")]
    SierraVersionFormat(StarknetApiError),
}

impl From<SierraCompilerError> for apollo_compile_to_casm_types::SierraCompilerError {
    fn from(error: SierraCompilerError) -> Self {
        apollo_compile_to_casm_types::SierraCompilerError::CompilationFailed(error.to_string())
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
    #[sequencer_latency_histogram(COMPILATION_DURATION, true)]
    pub fn compile(&self, class: RawClass) -> SierraCompilerResult<RawExecutableHashedClass> {
        let class = SierraContractClass::try_from(class)?;
        let sierra_version = SierraVersion::extract_from_program(&class.sierra_program)
            .map_err(SierraCompilerError::SierraVersionFormat)?;
        let class = into_contract_class_for_compilation(&class);

        // TODO(Elin): handle resources (whether here or an infra. layer load-balancing).
        let executable_class = self.compiler.compile(class)?;
        // TODO(Elin): consider spawning a worker for hash calculatioln.
        // TODO(Aviv): Get the compiled_class_hash_v2.
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

#[async_trait]
impl ComponentStarter for SierraCompiler {
    async fn start(&mut self) {
        default_component_start_fn::<Self>().await;
        register_metrics();
    }
}
