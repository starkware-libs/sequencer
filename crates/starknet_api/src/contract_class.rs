use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContractClass {
    V1(ContractClassV1),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContractClassV1 {
    Casm(CasmContractClass),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassInfo {
    pub contract_class: ContractClass,
    pub sierra_program_length: usize,
    pub abi_length: usize,
}
