use cairo_lang_starknet_classes::allowed_libfuncs::{AllowedLibfuncsError, ListSelector};
use cairo_lang_starknet_classes::casm_contract_class::{
    CasmContractClass, StarknetSierraCompilationError,
};
use cairo_lang_starknet_classes::contract_class::ContractClass;
use thiserror::Error;

#[cfg(test)]
#[path = "compile_test.rs"]
pub mod compile_test;
pub struct SierraToCasmCompilationArgs {
    list_selector: ListSelector,
    add_pythonic_hints: bool,
    max_bytecode_size: usize,
}

#[derive(Debug, Error)]
pub enum CompilationUtilError {
    #[error(transparent)]
    AllowedLibfuncsError(#[from] AllowedLibfuncsError),
    #[error(transparent)]
    StarknetSierraCompilationError(#[from] StarknetSierraCompilationError),
    #[error("Compilation panicked")]
    CompilationPanic,
}

/// This function may panic.
pub fn compile_sierra_to_casm(
    contract_class: ContractClass,
) -> Result<CasmContractClass, CompilationUtilError> {
    let compilation_args = SierraToCasmCompilationArgs {
        list_selector: ListSelector::DefaultList,
        add_pythonic_hints: true,
        max_bytecode_size: 1000000,
    };

    contract_class.validate_version_compatible(compilation_args.list_selector)?;

    Ok(CasmContractClass::from_contract_class(
        contract_class,
        compilation_args.add_pythonic_hints,
        compilation_args.max_bytecode_size,
    )?)
}
