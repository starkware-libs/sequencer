use std::sync::Arc;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass as CairoLangContractClass;
use starknet_api::contract_class::{ClassInfo, ContractClass, SierraVersion};
use starknet_api::rpc_transaction::RpcDeclareTransaction;
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_sierra_compile::command_line_compiler::CommandLineCompiler;
use starknet_sierra_compile::config::SierraToCasmCompilationConfig;
use starknet_sierra_compile::utils::into_contract_class_for_compilation;
use starknet_sierra_compile::SierraToCasmCompiler;
use tracing::{debug, error};

use crate::errors::GatewayResult;

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

    /// Formats the contract class for compilation, compiles it, and returns the compiled contract
    /// class wrapped in a [`ClassInfo`].
    /// Assumes the contract class is of a Sierra program which is compiled to Casm.
    pub(crate) fn process_declare_tx(
        &self,
        declare_tx: &RpcDeclareTransaction,
    ) -> GatewayResult<ClassInfo> {
        let RpcDeclareTransaction::V3(tx) = declare_tx;
        let rpc_contract_class = &tx.contract_class;
        let cairo_lang_contract_class = into_contract_class_for_compilation(rpc_contract_class);

        let casm_contract_class = self.compile(cairo_lang_contract_class)?;

        let sierra_version = SierraVersion::extract_from_program(
            &rpc_contract_class.sierra_program,
        )
        .map_err(|e| GatewaySpecError::ExtractSierraVersionError { data: (e.to_string()) })?;

        Ok(ClassInfo {
            contract_class: ContractClass::V1(casm_contract_class),
            sierra_program_length: rpc_contract_class.sierra_program.len(),
            abi_length: rpc_contract_class.abi.len(),
            sierra_version,
        })
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
