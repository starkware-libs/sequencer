use std::sync::LazyLock;

use starknet_api::contract_class::ContractClass;

use crate::execution::contract_class::RunnableCompiledClass;

static TEST_CASM_CONTRACT_CLASS: LazyLock<RunnableCompiledClass> =
    LazyLock::new(|| ContractClass::test_casm_contract_class().try_into().unwrap());

static TEST_DEPRECATED_CASM_CONTRACT_CLASS: LazyLock<RunnableCompiledClass> =
    LazyLock::new(|| ContractClass::test_deprecated_casm_contract_class().try_into().unwrap());

impl RunnableCompiledClass {
    pub fn test_casm_contract_class() -> Self {
        TEST_CASM_CONTRACT_CLASS.clone()
    }

    pub fn test_deprecated_casm_contract_class() -> Self {
        TEST_DEPRECATED_CASM_CONTRACT_CLASS.clone()
    }
}
