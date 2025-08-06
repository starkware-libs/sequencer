use std::borrow::Cow;
use std::ops::Deref;
use std::sync::Arc;

use cairo_lang_starknet_classes::NestedIntList;
use cairo_native::executor::AotContractExecutor;
use starknet_api::contract_class::compiled_class_hash::HashableCompiledClass;
use starknet_api::core::EntryPointSelector;
use starknet_types_core::felt::Felt;

use crate::execution::contract_class::{CompiledClassV1, EntryPointV1, NestedMultipleIntList};
use crate::execution::entry_point::EntryPointTypeAndSelector;
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

    /// Initialize a compiled class for native.
    ///
    /// executor must be derived from sierra_program which in turn must be derived from
    /// sierra_contract_class.
    pub fn new(executor: AotContractExecutor, casm: CompiledClassV1) -> NativeCompiledClassV1 {
        let contract = NativeCompiledClassV1Inner::new(executor, casm);

        Self(Arc::new(contract))
    }

    pub fn get_entry_point(
        &self,
        entry_point: &EntryPointTypeAndSelector,
    ) -> Result<EntryPointV1, PreExecutionError> {
        self.casm.get_entry_point(entry_point)
    }

    pub fn casm(&self) -> CompiledClassV1 {
        self.casm.clone()
    }
}

impl HashableCompiledClass<EntryPointV1, NestedMultipleIntList> for NativeCompiledClassV1 {
    fn get_hashable_l1_entry_points(&self) -> &[EntryPointV1] {
        &self.casm.entry_points_by_type.l1_handler
    }

    fn get_hashable_external_entry_points(&self) -> &[EntryPointV1] {
        &self.casm.entry_points_by_type.external
    }

    fn get_hashable_constructor_entry_points(&self) -> &[EntryPointV1] {
        &self.casm.entry_points_by_type.constructor
    }

    fn get_bytecode(&self) -> Vec<Felt> {
        self.casm.get_bytecode()
    }

    // TODO(AvivG): Avoid unnecessary `NestedIntList` creation by having `HashableCompiledClass`
    // accept `NestedMultipleInt` via a shared trait.
    fn get_bytecode_segment_lengths(&self) -> Cow<'_, NestedMultipleIntList> {
        Cow::Borrowed(&self.casm.bytecode_segment_felt_sizes)
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

// The location where the compiled class is loaded into memory will not
// be the same therefore we exclude it from the comparison.
impl PartialEq for NativeCompiledClassV1Inner {
    fn eq(&self, other: &Self) -> bool {
        self.casm == other.casm
    }
}

impl Eq for NativeCompiledClassV1Inner {}
