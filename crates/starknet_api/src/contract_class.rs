use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContractClass {
    V1(ContractClassV1),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContractClassV1 {
    Casm(CasmContractClass),
}

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassInfo {
    contract_class: ContractClass,
    sierra_program_length: usize,
    abi_length: usize,
}
