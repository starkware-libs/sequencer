//! A lib for compiling Sierra into Casm.
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass;

use crate::errors::CompilationUtilError;

pub mod build_utils;
pub mod cairo_lang_compiler;
pub mod command_line_compiler;
pub mod config;
pub mod errors;
pub mod utils;

#[cfg(test)]
pub mod test_utils;

#[cfg(test)]
#[path = "compile_test.rs"]
pub mod compile_test;

pub trait SierraToCasmCompiler: Send + Sync {
    fn compile(
        &self,
        contract_class: ContractClass,
    ) -> Result<CasmContractClass, CompilationUtilError>;
}
