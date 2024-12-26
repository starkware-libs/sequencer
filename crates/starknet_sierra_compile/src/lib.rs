//! A lib for compiling Sierra into Casm.
use cairo_lang_starknet_classes::casm_contract_class::{
    CasmContractClass,
    CasmContractEntryPoints,
};
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
pub mod resource_limits;
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
    ClassSerializationError(#[from] bincode::Error), // serde_json::Error), // bincode::Error),
    #[error(transparent)]
    ClassSerializationError_(#[from] serde_json::Error), // bincode::Error),
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
        let class: SierraContractClass = serde_json::from_slice(class.as_ref())?;
        dbg!("got here");
        let compiler_compatible_class = into_contract_class_for_compilation(&class);
        dbg!("got here 1");

        let sierra_version = SierraVersion::extract_from_program(&class.sierra_program)
            .map_err(|error| SierraCompilerError::SierraFormatError(error.to_string()))?;
        dbg!("got here 2");

        let mut executable_class_in = self.compiler.compile(compiler_compatible_class)?;
        // executable_class_in.bytecode_segment_lengths = None;
        executable_class_in.hints = vec![];
        executable_class_in.bytecode = vec![];
        executable_class_in.pythonic_hints = Some(vec![]);
        executable_class_in.entry_points_by_type = CasmContractEntryPoints::default();
        dbg!(executable_class_in.clone());
        dbg!("got here 3");
        let executable_class = ContractClass::V1((executable_class_in.clone(), sierra_version));
        let executable_class_hash = executable_class.compiled_class_hash();
        let executable_class_raw = bincode::serialize(&executable_class_in)?;
        dbg!(executable_class_raw.clone());
        dbg!(RawClass::from(executable_class_raw.clone()));
        dbg!("got here 4");
        let ContractClass::V1((executable_class_, _)) = executable_class else {
            panic!("hmm");
        };
        dbg!(executable_class_.bytecode_segment_lengths.clone());
        // TODO: consider spawning a worker for hash calculation.
        dbg!("got here 5");
        // let executable_class = serde_json::to_vec(&executable_class)?;
        dbg!("got here 6");

        Ok((RawClass::from(executable_class_raw), executable_class_hash))
    }
}
