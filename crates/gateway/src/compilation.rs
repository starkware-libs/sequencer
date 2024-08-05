use blockifier::execution::contract_class::{ClassInfo, ContractClass, ContractClassV1};
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass as CairoLangContractClass;
use starknet_api::core::CompiledClassHash;
use starknet_api::rpc_transaction::RpcDeclareTransaction;
use starknet_sierra_compile::compile::SierraToCasmCompiler;
use starknet_sierra_compile::utils::into_contract_class_for_compilation;

use crate::errors::{GatewayError, GatewayResult};

#[cfg(test)]
#[path = "compilation_test.rs"]
mod compilation_test;

// TODO(Arni): Pass the compiler with dependancy injection.
#[derive(Clone)]
pub struct GatewayCompiler {
    pub sierra_to_casm_compiler: SierraToCasmCompiler,
}

impl GatewayCompiler {
    /// Formats the contract class for compilation, compiles it, and returns the compiled contract
    /// class wrapped in a [`ClassInfo`].
    /// Assumes the contract class is of a Sierra program which is compiled to Casm.
    pub fn process_declare_tx(
        &self,
        declare_tx: &RpcDeclareTransaction,
    ) -> GatewayResult<ClassInfo> {
        let RpcDeclareTransaction::V3(tx) = declare_tx;
        let rpc_contract_class = &tx.contract_class;
        let cairo_lang_contract_class = into_contract_class_for_compilation(rpc_contract_class);

        let casm_contract_class = self.compile(cairo_lang_contract_class)?;

        validate_compiled_class_hash(&casm_contract_class, &tx.compiled_class_hash)?;

        Ok(ClassInfo::new(
            &ContractClass::V1(ContractClassV1::try_from(casm_contract_class)?),
            rpc_contract_class.sierra_program.len(),
            rpc_contract_class.abi.len(),
        )?)
    }

    // TODO(Arni): Pass the compilation args from the config.
    fn compile(
        &self,
        cairo_lang_contract_class: CairoLangContractClass,
    ) -> Result<CasmContractClass, GatewayError> {
        Ok(self.sierra_to_casm_compiler.compile_sierra_to_casm(cairo_lang_contract_class)?)
    }
}

/// Validates that the compiled class hash of the compiled contract class matches the supplied
/// compiled class hash.
fn validate_compiled_class_hash(
    casm_contract_class: &CasmContractClass,
    supplied_compiled_class_hash: &CompiledClassHash,
) -> Result<(), GatewayError> {
    let compiled_class_hash = CompiledClassHash(casm_contract_class.compiled_class_hash());
    if compiled_class_hash != *supplied_compiled_class_hash {
        return Err(GatewayError::CompiledClassHashMismatch {
            supplied: *supplied_compiled_class_hash,
            hash_result: compiled_class_hash,
        });
    }
    Ok(())
}
