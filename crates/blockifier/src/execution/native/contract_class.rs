use std::ops::Deref;
use std::sync::Arc;

use cairo_native::executor::AotContractExecutor;
use starknet_api::core::EntryPointSelector;

use crate::execution::contract_class::{CompiledClassV1, EntryPointV1};
use crate::execution::entry_point::CallEntryPoint;
use crate::execution::errors::PreExecutionError;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeCompiledClassV1(pub Arc<NativeCompiledClassV1Inner>);
impl Deref for NativeCompiledClassV1 {
    type Target = NativeCompiledClassV1Inner;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl NativeCompiledClassV1 {
    pub(crate) fn constructor_selector(&self) -> Option<EntryPointSelector> {
        self.casm.constructor_selector()
    }

    /// Initialize a compiled contract class for native.
    ///
    /// executor must be derived from sierra_program which in turn must be derived from
    /// sierra_contract_class.
    pub fn new(executor: AotContractExecutor, casm: CompiledClassV1) -> NativeCompiledClassV1 {
        let contract = NativeCompiledClassV1Inner::new(executor, casm);

        Self(Arc::new(contract))
    }

    pub fn get_entry_point(
        &self,
        call: &CallEntryPoint,
    ) -> Result<EntryPointV1, PreExecutionError> {
        self.casm.get_entry_point(call)
    }

    pub fn casm(&self) -> CompiledClassV1 {
        self.casm.clone()
    }
}

#[derive(Debug)]
pub struct NativeCompiledClassV1Inner {
    pub executor: AotContractExecutor,
    casm: CompiledClassV1,
}

impl NativeCompiledClassV1Inner {
    fn new(executor: AotContractExecutor, casm: CompiledClassV1) -> Self {
        NativeCompiledClassV1Inner { executor, casm }
    }
}

// The location where the compiled contract is loaded into memory will not
// be the same therefore we exclude it from the comparison.
impl PartialEq for NativeCompiledClassV1Inner {
    fn eq(&self, other: &Self) -> bool {
        self.casm == other.casm
    }
}

impl Eq for NativeCompiledClassV1Inner {}
