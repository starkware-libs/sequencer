use std::sync::Arc;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass as CairoLangContractClass;
use starknet_api::contract_class::ClassInfo;
use starknet_api::core::{ChainId, ClassHash, CompiledClassHash};
use starknet_api::executable_transaction::DeclareTransaction;
use starknet_api::rpc_transaction::RpcDeclareTransaction;
use starknet_api::transaction::{DeclareTransactionV3, TransactionHasher};
use starknet_sierra_compile::cairo_lang_compiler::CairoLangSierraToCasmCompiler;
use starknet_sierra_compile::config::SierraToCasmCompilationConfig;
use starknet_sierra_compile::utils::into_contract_class_for_compilation;
use starknet_sierra_compile::SierraToCasmCompiler;
use tracing::{debug, error};

use crate::errors::{GatewayResult, GatewaySpecError};

#[cfg(test)]
#[path = "compilation_test.rs"]
mod compilation_test;

// TODO(Arni): Pass the compiler with dependancy injection.
#[derive(Clone)]
pub struct GatewayCompiler {
    pub sierra_to_casm_compiler: Arc<dyn SierraToCasmCompiler>,
}

impl GatewayCompiler {
    pub fn new_cairo_lang_compiler(config: SierraToCasmCompilationConfig) -> Self {
        Self { sierra_to_casm_compiler: Arc::new(CairoLangSierraToCasmCompiler { config }) }
    }

    // TODO(Arni): Squash this function into `process_declare_tx`.
    /// Formats the contract class for compilation, compiles it, and returns the compiled contract
    /// class wrapped in a [`ClassInfo`].
    /// Assumes the contract class is of a Sierra program which is compiled to Casm.
    pub fn class_info_from_declare_tx(
        &self,
        declare_tx: &RpcDeclareTransaction,
    ) -> GatewayResult<ClassInfo> {
        let RpcDeclareTransaction::V3(tx) = declare_tx;
        let rpc_contract_class = &tx.contract_class;
        let cairo_lang_contract_class = into_contract_class_for_compilation(rpc_contract_class);

        let casm_contract_class = self.compile(cairo_lang_contract_class)?;

        validate_compiled_class_hash(&casm_contract_class, &tx.compiled_class_hash)?;

        Ok(ClassInfo {
            casm_contract_class,
            sierra_program_length: rpc_contract_class.sierra_program.len(),
            abi_length: rpc_contract_class.abi.len(),
        })
    }

    /// Processes a declare transaction, compiling the contract class and returning the executable
    /// declare transaction.
    pub fn process_declare_tx(
        &self,
        rpc_tx: &RpcDeclareTransaction,
        chain_id: &ChainId,
    ) -> GatewayResult<DeclareTransaction> {
        let class_info = self.class_info_from_declare_tx(rpc_tx)?;
        let RpcDeclareTransaction::V3(tx) = rpc_tx;
        let declare_tx = starknet_api::transaction::DeclareTransaction::V3(DeclareTransactionV3 {
            class_hash: ClassHash::default(), /* FIXME(yael 15/4/24): call the starknet-api
                                               * function once ready */
            resource_bounds: tx.resource_bounds.clone().into(),
            tip: tx.tip,
            signature: tx.signature.clone(),
            nonce: tx.nonce,
            compiled_class_hash: tx.compiled_class_hash,
            sender_address: tx.sender_address,
            nonce_data_availability_mode: tx.nonce_data_availability_mode,
            fee_data_availability_mode: tx.fee_data_availability_mode,
            paymaster_data: tx.paymaster_data.clone(),
            account_deployment_data: tx.account_deployment_data.clone(),
        });
        let tx_hash = declare_tx
            .calculate_transaction_hash(chain_id, &declare_tx.version())
            .map_err(|err| {
                error!("Failed to calculate tx hash: {}", err);
                GatewaySpecError::UnexpectedError { data: "Internal server error.".to_owned() }
            })?;
        Ok(DeclareTransaction { tx: declare_tx, tx_hash, class_info })
    }

    fn compile(
        &self,
        cairo_lang_contract_class: CairoLangContractClass,
    ) -> GatewayResult<CasmContractClass> {
        match self.sierra_to_casm_compiler.compile(cairo_lang_contract_class) {
            Ok(casm_contract_class) => Ok(casm_contract_class),
            Err(starknet_sierra_compile::errors::CompilationUtilError::CompilationPanic) => {
                // TODO(Arni): Log the panic.
                error!("Compilation panicked.");
                Err(GatewaySpecError::UnexpectedError { data: "Internal server error.".to_owned() })
            }
            Err(e) => {
                debug!("Compilation failed: {:?}", e);
                Err(GatewaySpecError::CompilationFailed)
            }
        }
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
