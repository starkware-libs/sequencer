use std::panic;

use cairo_lang_starknet_classes::allowed_libfuncs::ListSelector;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass;
use starknet_sierra_compile_types::errors::CompilationUtilError;

use crate::config::SierraToCasmCompilationConfig;
use crate::sierra_to_casm_compiler::SierraToCasmCompiler;

#[cfg(test)]
#[path = "compile_test.rs"]
pub mod compile_test;

/// A compiler that compiles Sierra programs to Casm. Uses the code from the
/// `cairo_lang_starknet_classes` crate.
#[derive(Clone)]
pub struct CairoLangSierraToCasmCompiler {
    pub config: SierraToCasmCompilationConfig,
}

impl SierraToCasmCompiler for CairoLangSierraToCasmCompiler {
    fn compile(
        &self,
        contract_class: ContractClass,
    ) -> Result<CasmContractClass, CompilationUtilError> {
        let catch_unwind_result = panic::catch_unwind(|| self.compile_inner(contract_class));
        let casm_contract_class =
            catch_unwind_result.map_err(|_| CompilationUtilError::CompilationPanic)??;

        Ok(casm_contract_class)
    }
}

impl CairoLangSierraToCasmCompiler {
    fn compile_inner(
        &self,
        contract_class: ContractClass,
    ) -> Result<CasmContractClass, CompilationUtilError> {
        contract_class.validate_version_compatible(ListSelector::DefaultList)?;

        Ok(CasmContractClass::from_contract_class(
            contract_class,
            true,
            self.config.max_bytecode_size,
        )?)
    }
}
