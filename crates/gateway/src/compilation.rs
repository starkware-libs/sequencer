use std::panic;
use std::sync::OnceLock;

use blockifier::execution::contract_class::{ClassInfo, ContractClass, ContractClassV1};
use cairo_lang_starknet_classes::casm_contract_class::{
    CasmContractClass, CasmContractEntryPoints,
};
use starknet_api::core::CompiledClassHash;
use starknet_api::rpc_transaction::RPCDeclareTransaction;
use starknet_sierra_compile::compile::compile_sierra_to_casm;
use starknet_sierra_compile::errors::CompilationUtilError;
use starknet_sierra_compile::utils::into_contract_class_for_compilation;

use crate::config::GatewayCompilerConfig;
use crate::errors::{GatewayError, GatewayResult};
use crate::utils::is_subsequence;

#[cfg(test)]
#[path = "compilation_test.rs"]
mod compilation_test;

#[derive(Clone)]
pub struct GatewayCompiler {
    #[allow(dead_code)]
    pub config: GatewayCompilerConfig,
}

impl GatewayCompiler {
    /// Formats the contract class for compilation, compiles it, and returns the compiled contract
    /// class wrapped in a [`ClassInfo`].
    /// Assumes the contract class is of a Sierra program which is compiled to Casm.
    pub fn compile_contract_class(
        &self,
        declare_tx: &RPCDeclareTransaction,
    ) -> GatewayResult<ClassInfo> {
        let RPCDeclareTransaction::V3(tx) = declare_tx;
        let starknet_api_contract_class = &tx.contract_class;
        let cairo_lang_contract_class =
            into_contract_class_for_compilation(starknet_api_contract_class);

        // Compile Sierra to Casm.
        let catch_unwind_result =
            panic::catch_unwind(|| compile_sierra_to_casm(cairo_lang_contract_class));
        let casm_contract_class = match catch_unwind_result {
            Ok(compilation_result) => compilation_result?,
            Err(_) => {
                // TODO(Arni): Log the panic.
                return Err(GatewayError::CompilationError(CompilationUtilError::CompilationPanic));
            }
        };
        self.validate_casm_class(&casm_contract_class)?;

        let hash_result = CompiledClassHash(casm_contract_class.compiled_class_hash());
        if hash_result != tx.compiled_class_hash {
            return Err(GatewayError::CompiledClassHashMismatch {
                supplied: tx.compiled_class_hash,
                hash_result,
            });
        }

        // Convert Casm contract class to Starknet contract class directly.
        let blockifier_contract_class =
            ContractClass::V1(ContractClassV1::try_from(casm_contract_class)?);
        let class_info = ClassInfo::new(
            &blockifier_contract_class,
            starknet_api_contract_class.sierra_program.len(),
            starknet_api_contract_class.abi.len(),
        )?;
        Ok(class_info)
    }

    // TODO(Arni): Add test.
    fn validate_casm_class(&self, contract_class: &CasmContractClass) -> Result<(), GatewayError> {
        let CasmContractEntryPoints { external, l1_handler, constructor } =
            &contract_class.entry_points_by_type;
        let entry_points_iterator =
            external.iter().chain(l1_handler.iter()).chain(constructor.iter());

        for entry_point in entry_points_iterator {
            let builtins = &entry_point.builtins;
            if !is_subsequence(builtins, supported_builtins()) {
                return Err(GatewayError::UnsupportedBuiltins {
                    builtins: builtins.clone(),
                    supported_builtins: supported_builtins().to_vec(),
                });
            }
        }
        Ok(())
    }
}

// TODO(Arni): Add to a config.
// TODO(Arni): Use the Builtin enum from Starknet-api, and explicitly tag each builtin as supported
// or unsupported so that the compiler would alert us on new builtins.
fn supported_builtins() -> &'static Vec<String> {
    static SUPPORTED_BUILTINS: OnceLock<Vec<String>> = OnceLock::new();
    SUPPORTED_BUILTINS.get_or_init(|| {
        // The OS expects this order for the builtins.
        const SUPPORTED_BUILTIN_NAMES: [&str; 7] =
            ["pedersen", "range_check", "ecdsa", "bitwise", "ec_op", "poseidon", "segment_arena"];
        SUPPORTED_BUILTIN_NAMES.iter().map(|builtin| builtin.to_string()).collect::<Vec<String>>()
    })
}
