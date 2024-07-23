use std::panic;
use std::sync::OnceLock;

use blockifier::execution::contract_class::{ClassInfo, ContractClass, ContractClassV1};
use cairo_lang_starknet_classes::casm_contract_class::{
    CasmContractClass, CasmContractEntryPoints,
};
use cairo_lang_starknet_classes::contract_class::ContractClass as CairoLangContractClass;
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
        declare_tx: &RPCDeclareTransaction,
    ) -> GatewayResult<ClassInfo> {
        let RPCDeclareTransaction::V3(tx) = declare_tx;
        let rpc_contract_class = &tx.contract_class;
        let cairo_lang_contract_class = into_contract_class_for_compilation(rpc_contract_class);

        let casm_contract_class = self.compile(cairo_lang_contract_class)?;

        validate_compiled_class_hash(&casm_contract_class, &tx.compiled_class_hash)?;
        validate_casm_class(&casm_contract_class)?;

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
        let catch_unwind_result =
            panic::catch_unwind(|| compile_sierra_to_casm(cairo_lang_contract_class));
        let casm_contract_class =
            catch_unwind_result.map_err(|_| CompilationUtilError::CompilationPanic)??;

        Ok(casm_contract_class)
    }
}

// TODO(Arni): Add test.
fn validate_casm_class(contract_class: &CasmContractClass) -> Result<(), GatewayError> {
    let CasmContractEntryPoints { external, l1_handler, constructor } =
        &contract_class.entry_points_by_type;
    let entry_points_iterator = external.iter().chain(l1_handler.iter()).chain(constructor.iter());

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
