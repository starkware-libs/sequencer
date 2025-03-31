//! A lib for compiling Sierra into Casm.
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass;
#[cfg(feature = "cairo_native")]
use cairo_native::executor::AotContractExecutor;

use crate::errors::CompilationUtilError;

pub mod command_line_compiler;
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

#[cfg(test)]
#[path = "constants_test.rs"]
pub mod constants_test;

pub trait SierraToCasmCompiler: Send + Sync {
    fn compile(
        &self,
        contract_class: ContractClass,
    ) -> Result<CasmContractClass, CompilationUtilError>;
}

#[cfg(feature = "cairo_native")]
pub trait SierraToNativeCompiler: Send + Sync {
    fn compile_to_native(
        &self,
        contract_class: ContractClass,
    ) -> Result<AotContractExecutor, CompilationUtilError>;
}
