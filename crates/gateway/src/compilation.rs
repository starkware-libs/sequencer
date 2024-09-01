use std::sync::Arc;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass as CairoLangContractClass;
use starknet_api::contract_class::ClassInfo;
use starknet_api::core::ChainId;
use starknet_api::executable_transaction::DeclareTransaction;
use starknet_api::rpc_transaction::RpcDeclareTransaction;
use starknet_sierra_compile::cairo_lang_compiler::CairoLangSierraToCasmCompiler;
use starknet_sierra_compile::command_line_compiler::CommandLineCompiler;
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
    pub fn new_command_line_compiler(config: SierraToCasmCompilationConfig) -> Self {
        Self { sierra_to_casm_compiler: Arc::new(CommandLineCompiler::new(config)) }
    }

    // TODO(Arni): Cosider deleting `CairoLangSierraToCasmCompiler`.
    pub fn new_cairo_lang_compiler(config: SierraToCasmCompilationConfig) -> Self {
        Self { sierra_to_casm_compiler: Arc::new(CairoLangSierraToCasmCompiler { config }) }
    }

    // TODO(Arni): Squash this function into `process_declare_tx`.
    /// Formats the contract class for compilation, compiles it, and returns the compiled contract
    /// class wrapped in a [`ClassInfo`].
    /// Assumes the contract class is of a Sierra program which is compiled to Casm.
    pub(crate) fn class_info_from_declare_tx(
        &self,
        declare_tx: &RpcDeclareTransaction,
    ) -> GatewayResult<ClassInfo> {
        let RpcDeclareTransaction::V3(tx) = declare_tx;
        let rpc_contract_class = &tx.contract_class;
        let cairo_lang_contract_class = into_contract_class_for_compilation(rpc_contract_class);

        let casm_contract_class = self.compile(cairo_lang_contract_class)?;

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
        rpc_tx: RpcDeclareTransaction,
        chain_id: &ChainId,
    ) -> GatewayResult<DeclareTransaction> {
        let class_info = self.class_info_from_declare_tx(&rpc_tx)?;
        let declare_tx: starknet_api::transaction::DeclareTransaction = rpc_tx.into();
        let executable_declare_tx = DeclareTransaction::create(declare_tx, class_info, chain_id)
            .map_err(|err| {
                debug!("Failed to create executable declare transaction {:?}", err);
                GatewaySpecError::UnexpectedError { data: "Internal server error.".to_owned() }
            })?;

        if !executable_declare_tx.validate_compiled_class_hash() {
            return Err(GatewaySpecError::CompiledClassHashMismatch);
        }

        Ok(executable_declare_tx)
    }

    fn compile(
        &self,
        cairo_lang_contract_class: CairoLangContractClass,
    ) -> GatewayResult<CasmContractClass> {
        match self.sierra_to_casm_compiler.compile(cairo_lang_contract_class) {
            Ok(casm_contract_class) => Ok(casm_contract_class),
            Err(starknet_sierra_compile::errors::CompilationUtilError::UnexpectedError(error)) => {
                error!("Compilation panicked. Error: {:?}", error);
                Err(GatewaySpecError::UnexpectedError { data: "Internal server error.".to_owned() })
            }
            Err(e) => {
                debug!("Compilation failed: {:?}", e);
                Err(GatewaySpecError::CompilationFailed)
            }
        }
    }
}
