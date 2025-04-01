//! A lib for compiling Sierra into Casm.
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass;
use starknet_compilation_utils::errors::CompilationUtilError;

pub mod command_line_compiler;
pub mod config;
pub mod constants;

#[cfg(test)]
#[path = "compile_test.rs"]
pub mod compile_test;

pub trait SierraToCasmCompiler: Send + Sync {
    fn compile(
        &self,
        contract_class: ContractClass,
    ) -> Result<CasmContractClass, CompilationUtilError>;
}
