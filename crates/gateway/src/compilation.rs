use std::panic;

use blockifier::execution::contract_class::{ClassInfo, ContractClass, ContractClassV1};
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass as CairoLangContractClass;
use starknet_api::core::CompiledClassHash;
use starknet_api::rpc_transaction::RpcDeclareTransaction;
use starknet_sierra_compile::compile::compile_sierra_to_casm;
use starknet_sierra_compile::utils::into_contract_class_for_compilation;
use tracing::{debug, error};

use crate::config::GatewayCompilerConfig;
use crate::errors::{GatewayResult, GatewaySpecError};

#[cfg(test)]
#[path = "compilation_test.rs"]
mod compilation_test;

// TODO(Arni): Pass the compiler with dependancy injection.
#[derive(Clone)]
pub struct GatewayCompiler {
    #[allow(dead_code)]
    pub config: GatewayCompilerConfig,
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

        ClassInfo::new(
            &ContractClass::V1(ContractClassV1::try_from(casm_contract_class).map_err(|e| {
                error!("Failed to convert CasmContractClass to Blockifier ContractClass: {:?}", e);
                GatewaySpecError::UnexpectedError { data: "Internal server error.".to_owned() }
            })?),
            rpc_contract_class.sierra_program.len(),
            rpc_contract_class.abi.len(),
        )
        .map_err(|e| {
            error!("Failed to convert Blockifier ContractClass to Blockifier ClassInfo: {:?}", e);
            GatewaySpecError::UnexpectedError { data: "Internal server error.".to_owned() }
        })
    }

    // TODO(Arni): Pass the compilation args from the config.
    fn compile(
        &self,
        cairo_lang_contract_class: CairoLangContractClass,
    ) -> GatewayResult<CasmContractClass> {
        let catch_unwind_result =
            panic::catch_unwind(|| compile_sierra_to_casm(cairo_lang_contract_class));
        let casm_contract_class = match catch_unwind_result {
            Ok(compilation_result) => compilation_result.map_err(|e| {
                debug!("Compilation failed: {:?}", e);
                GatewaySpecError::CompilationFailed
            })?,
            Err(_panicked_compilation) => {
                // TODO(Arni): Log the panic.
                error!("Compilation panicked.");
                return Err(GatewaySpecError::UnexpectedError {
                    data: "Internal server error.".to_owned(),
                });
            }
        };

        Ok(casm_contract_class)
    }
}

/// Validates that the compiled class hash of the compiled contract class matches the supplied
/// compiled class hash.
fn validate_compiled_class_hash(
    casm_contract_class: &CasmContractClass,
    supplied_compiled_class_hash: &CompiledClassHash,
) -> GatewayResult<()> {
    let compiled_class_hash = CompiledClassHash(casm_contract_class.compiled_class_hash());
    if compiled_class_hash != *supplied_compiled_class_hash {
        debug!(
            "Compiled class hash mismatch. Supplied: {:?}, Hash result: {:?}",
            supplied_compiled_class_hash, compiled_class_hash
        );
        return Err(GatewaySpecError::CompiledClassHashMismatch);
    }
    Ok(())
}
