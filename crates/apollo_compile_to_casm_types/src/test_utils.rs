use std::sync::LazyLock;

use starknet_api::contract_class::ContractClass;

use crate::RawExecutableClass;

static TEST_CASM_CONTRACT_CLASS: LazyLock<RawExecutableClass> =
    LazyLock::new(|| ContractClass::test_casm_contract_class().try_into().unwrap());

static TEST_DEPRECATED_CASM_CONTRACT_CLASS: LazyLock<RawExecutableClass> =
    LazyLock::new(|| ContractClass::test_deprecated_casm_contract_class().try_into().unwrap());

impl RawExecutableClass {
    pub fn test_casm_contract_class() -> Self {
        TEST_CASM_CONTRACT_CLASS.clone()
    }

    pub fn test_deprecated_casm_contract_class() -> Self {
        TEST_DEPRECATED_CASM_CONTRACT_CLASS.clone()
    }
}
