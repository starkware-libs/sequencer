use std::ops::Deref;
use std::sync::Arc;

use cairo_native::executor::AotContractExecutor;
use starknet_api::core::EntryPointSelector;

use crate::execution::contract_class::ContractClassV1;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeContractClassV1(pub Arc<NativeContractClassV1Inner>);
impl Deref for NativeContractClassV1 {
    type Target = NativeContractClassV1Inner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl NativeContractClassV1 {
    pub(crate) fn constructor_selector(&self) -> Option<EntryPointSelector> {
        self.casm.entry_points_by_type.constructor.first().map(|ep| ep.selector)
    }

    /// Initialize a compiled contract class for native.
    ///
    /// executor must be derived from sierra_program which in turn must be derived from
    /// sierra_contract_class.
    pub fn new(executor: AotContractExecutor, casm: ContractClassV1) -> NativeContractClassV1 {
        let contract = NativeContractClassV1Inner::new(executor, casm);

        Self(Arc::new(contract))
    }

    pub fn casm(&self) -> ContractClassV1 {
        self.casm.clone()
    }
}

#[derive(Debug)]
pub struct NativeContractClassV1Inner {
    pub executor: AotContractExecutor,
    casm: ContractClassV1,
}

impl NativeContractClassV1Inner {
    fn new(executor: AotContractExecutor, casm: ContractClassV1) -> Self {
        NativeContractClassV1Inner { executor, casm }
    }
}

// The location where the compiled contract is loaded into memory will not
// be the same therefore we exclude it from the comparison.
impl PartialEq for NativeContractClassV1Inner {
    fn eq(&self, other: &Self) -> bool {
        self.casm == other.casm
    }
}

impl Eq for NativeContractClassV1Inner {}
