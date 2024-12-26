//! A lib for compiling Sierra into Casm.
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass as CairoLangContractClass;
#[cfg(feature = "cairo_native")]
use cairo_native::executor::AotContractExecutor;
use starknet_api::contract_class::{ContractClass, SierraVersion};
use starknet_api::state::SierraContractClass;
use starknet_sierra_compile_types::{RawClass, RawExecutableClass};
use thiserror::Error;
use utils::into_contract_class_for_compilation;

use crate::errors::CompilationUtilError;

pub mod command_line_compiler;
pub mod communication;
pub mod config;
pub mod constants;
pub mod errors;
pub mod paths;
pub mod utils;

#[cfg(test)]
pub mod test_utils;

#[cfg(test)]
#[path = "compile_test.rs"]
pub mod compile_test;

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
    ClassSerializationError(#[from] bincode::Error),
    #[error(transparent)]
    CompilationError(#[from] CompilationUtilError),
    #[error("Failed to parse Sierra: {0}")]
    SierraFormatError(String),
}

type SierraCompilerResult<T> = Result<T, SierraCompilerError>;

struct SierraCompiler<C: SierraToCasmCompiler> {
    compiler: C,
}

impl<C: SierraToCasmCompiler> SierraCompiler<C> {
    pub fn new(compiler: C) -> Self {
        Self { compiler }
    }

    // TODO: move (de)serialization to infra. layer.
    fn compile(&self, class: RawClass) -> SierraCompilerResult<RawExecutableClass> {
        let class: SierraContractClass = bincode::deserialize(&class[..])?;
        let compiler_compatible_class = into_contract_class_for_compilation(&class);

        let sierra_version = SierraVersion::extract_from_program(&class.sierra_program)
            .map_err(|error| SierraCompilerError::SierraFormatError(error.to_string()))?;

        let executable_class = self.compiler.compile(compiler_compatible_class)?;
        let executable_class = ContractClass::V1((executable_class, sierra_version));
        // TODO: consider spawning a worker for hash calculation.
        let executable_class_hash = executable_class.compiled_class_hash();
        let executable_class = bincode::serialize(&executable_class)?.into();

        Ok((executable_class, executable_class_hash))
    }
}
