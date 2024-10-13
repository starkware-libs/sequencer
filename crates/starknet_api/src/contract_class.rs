use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use serde::{Deserialize, Serialize};

use crate::core::CompiledClassHash;
use crate::deprecated_contract_class::ContractClass as DeprecatedContractClass;

#[derive(
    Debug, Default, Clone, Copy, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(deny_unknown_fields)]
pub enum EntryPointType {
    /// A constructor entry point.
    #[serde(rename = "CONSTRUCTOR")]
    Constructor,
    /// An external entry point.
    #[serde(rename = "EXTERNAL")]
    #[default]
    External,
    /// An L1 handler entry point.
    #[serde(rename = "L1_HANDLER")]
    L1Handler,
}

/// Represents a raw Starknet contract class.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ContractClass {
    V0(DeprecatedContractClass),
    V1(CasmContractClass),
}

impl ContractClass {
    pub fn compiled_class_hash(&self) -> CompiledClassHash {
        match self {
            ContractClass::V0(_) => panic!("Cairo 0 doesn't have compiled class hash."),
            ContractClass::V1(casm_contract_class) => {
                CompiledClassHash(casm_contract_class.compiled_class_hash())
            }
        }
    }
}
/// All relevant information about a declared contract class, including the compiled contract class
/// and other parameters derived from the original declare transaction required for billing.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClassInfo {
    // TODO(Noa): Consider using Arc.
    pub contract_class: ContractClass,
    pub sierra_program_length: usize,
    pub abi_length: usize,
}
