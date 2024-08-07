use std::sync::Arc;

use blockifier::execution::contract_class::{ClassInfo, ContractClass, ContractClassV1};
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass as CairoLangContractClass;
use starknet_api::core::CompiledClassHash;
use starknet_api::rpc_transaction::RpcDeclareTransaction;
use starknet_sierra_compile::cairo_lang_compiler::CairoLangSierraToCasmCompiler;
use starknet_sierra_compile::utils::into_contract_class_for_compilation;
use starknet_sierra_compile::SierraToCasmCompiler;

use crate::config::{GatewayCompilerConfig, PostCompilationConfig};
use crate::errors::{GatewayError, GatewayResult};

#[cfg(test)]
#[path = "compilation_test.rs"]
mod compilation_test;

// TODO(Arni): Pass the compiler with dependancy injection.
#[derive(Clone)]
pub struct GatewayCompiler {
    config: PostCompilationConfig,
    sierra_to_casm_compiler: Arc<dyn SierraToCasmCompiler>,
}

impl GatewayCompiler {
    pub fn new_cairo_lang_compiler(config: GatewayCompilerConfig) -> Self {
        Self {
            config: config.post_compilation_config,
            sierra_to_casm_compiler: Arc::new(CairoLangSierraToCasmCompiler {
                config: config.sierra_to_casm_compiler_config,
            }),
        }
    }

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
        self.validate_casm_class_size(&casm_contract_class)?;

        Ok(ClassInfo::new(
            &ContractClass::V1(ContractClassV1::try_from(casm_contract_class)?),
            rpc_contract_class.sierra_program.len(),
            rpc_contract_class.abi.len(),
        )?)
    }

    fn compile(
        &self,
        cairo_lang_contract_class: CairoLangContractClass,
    ) -> Result<CasmContractClass, GatewayError> {
        Ok(self.sierra_to_casm_compiler.compile(cairo_lang_contract_class)?)
    }

    // TODO(Arni): consider validating the size of other members of the Casm class. Cosider removing
    // the validation of the raw class size. The validation should be linked to the way the class is
    // saved in Papyrus etc.
    /// Validates that the Casm class is within size limit. Specifically, this function validates
    /// the size of the serialized class.
    fn validate_casm_class_size(
        &self,
        casm_contract_class: &CasmContractClass,
    ) -> Result<(), GatewayError> {
        let contract_class_object_size = serde_json::to_string(&casm_contract_class)
            .expect("Unexpected error serializing Casm contract class.")
            .len();
        if contract_class_object_size > self.config.max_casm_contract_class_object_size {
            return Err(GatewayError::CasmContractClassObjectSizeTooLarge {
                contract_class_object_size,
                max_contract_class_object_size: self.config.max_casm_contract_class_object_size,
            });
        }

        Ok(())
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
