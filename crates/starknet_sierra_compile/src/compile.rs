use cairo_lang_starknet_classes::allowed_libfuncs::ListSelector;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass;

use crate::config::SierraToCasmCompilationConfig;
use crate::errors::CompilationUtilError;

#[cfg(test)]
#[path = "compile_test.rs"]
pub mod compile_test;

#[derive(Clone)]
pub struct SierraToCasmCompiler {
    pub config: SierraToCasmCompilationConfig,
}

impl SierraToCasmCompiler {
    pub fn compile_sierra_to_casm(
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
