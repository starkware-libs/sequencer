use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use serde::{Deserialize, Serialize};

/// Compiled contract class.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ContractClass {
    V1(ContractClassV1),
}

/// Compiled contract class variant for Cairo 1 contracts.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ContractClassV1 {
    Casm(CasmContractClass),
}

/// All relevant information about a declared contract class, including the compiled contract class
/// and other parameters derived from the original declare transaction required for billing.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClassInfo {
    pub contract_class: ContractClass,
    pub sierra_program_length: usize,
    pub abi_length: usize,
}
