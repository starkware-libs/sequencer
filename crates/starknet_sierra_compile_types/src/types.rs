use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SierraToCasmCompilerInput {
    pub contract_class: ContractClass,
}

pub struct SierraToCasmCompilerOutput {
    pub casm_contract_class: CasmContractClass,
}
