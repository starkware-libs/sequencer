use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;

/// Compiled contract class.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContractClass {
    V1(ContractClassV1),
}

/// Compiled contract class variant for Cairo 1 contracts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContractClassV1 {
    Casm(CasmContractClass),
}

/// All relevant information about a declared contract class, including the compiled contract class
/// and other parameters derived from the original declare transaction required for billing.
#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassInfo {
    contract_class: ContractClass,
    sierra_program_length: usize,
    abi_length: usize,
}
