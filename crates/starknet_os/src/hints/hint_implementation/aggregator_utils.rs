use std::collections::HashMap;

use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::transaction::MessageToL1;
use starknet_types_core::felt::Felt;

use crate::io::os_output::{
    FullOsOutput,
    MessageToL2,
    MESSAGE_TO_L1_CONST_FIELD_SIZE,
    MESSAGE_TO_L2_CONST_FIELD_SIZE,
};
use crate::vm_utils::{IdentifierGetter, LoadCairoObject, VmUtilsResult};

pub(crate) trait ToMaybeRelocatables {
    fn to_maybe_relocatables(&self) -> Vec<MaybeRelocatable>;
}

impl ToMaybeRelocatables for MessageToL1 {
    fn to_maybe_relocatables(&self) -> Vec<MaybeRelocatable> {
        let mut res = Vec::<MaybeRelocatable>::with_capacity(
            MESSAGE_TO_L1_CONST_FIELD_SIZE + self.payload.0.len(),
        );
        res.push(Felt::from(self.from_address).into());
        res.push(Felt::from(self.to_address).into());
        res.push(Felt::from(self.payload.0.len()).into());
        res.extend(self.payload.0.iter().map(|felt| felt.into()));
        res
    }
}

impl ToMaybeRelocatables for MessageToL2 {
    fn to_maybe_relocatables(&self) -> Vec<MaybeRelocatable> {
        let mut res = Vec::<MaybeRelocatable>::with_capacity(
            MESSAGE_TO_L2_CONST_FIELD_SIZE + self.payload.0.len(),
        );
        res.push(Felt::from(self.from_address).into());
        res.push(Felt::from(self.to_address).into());
        res.push((self.nonce.0).into());
        res.push((self.selector.0).into());
        res.push(Felt::from(self.payload.0.len()).into());
        res.extend(self.payload.0.iter().map(|felt| felt.into()));
        res
    }
}

impl<IG: IdentifierGetter, T: ToMaybeRelocatables> LoadCairoObject<IG> for T {
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        _identifier_getter: &IG,
        address: Relocatable,
        _constants: &std::collections::HashMap<String, Felt>,
    ) -> VmUtilsResult<Relocatable> {
        Ok(vm.load_data(address, &self.to_maybe_relocatables())?)
    }
}

struct StateEntryManager {
    _state_entry_ptr: Relocatable,
    _storage_dict_ptr: Relocatable,
}

/// A utility struct to allow chaining diffs of the same contract that appears in different OsOutput
/// state diffs.
pub(crate) struct FullStateDiffWriter {
    _storage_dict_ptr: Relocatable,
    _class_dict_ptr: Relocatable,
    _inner_storage: HashMap<MaybeRelocatable, StateEntryManager>,
}

impl FullStateDiffWriter {
    pub(crate) fn new(vm: &mut VirtualMachine) -> Self {
        Self {
            _storage_dict_ptr: vm.add_memory_segment(),
            _class_dict_ptr: vm.add_memory_segment(),
            _inner_storage: HashMap::new(),
        }
    }

    pub(crate) fn _get_storage_dict_ptr(&self) -> Relocatable {
        self._storage_dict_ptr
    }

    pub(crate) fn _get_class_dict_ptr(&self) -> Relocatable {
        self._class_dict_ptr
    }
}

/// Writes the given `FullOsOutput` to the VM at the specified address.
fn write_full_os_output<IG: IdentifierGetter>(
    output: &FullOsOutput,
    vm: &mut VirtualMachine,
    identifier_getter: &IG,
    _address: Relocatable,
    constants: &std::collections::HashMap<String, Felt>,
    _state_diff_writer: &mut FullStateDiffWriter,
) -> VmUtilsResult<Relocatable> {
    let FullOsOutput { common_os_output, .. } = output;
    let messages_to_l1_start = vm.add_temporary_segment();
    let _messages_to_l1_end = common_os_output.messages_to_l1.load_into(
        vm,
        identifier_getter,
        messages_to_l1_start,
        constants,
    )?;

    let messages_to_l2_start = vm.add_temporary_segment();
    let _messages_to_l2_end = common_os_output.messages_to_l2.load_into(
        vm,
        identifier_getter,
        messages_to_l2_start,
        constants,
    )?;
    todo!()
}

pub(crate) struct FullOsOutputs(pub Vec<FullOsOutput>);

impl<IG: IdentifierGetter> LoadCairoObject<IG> for FullOsOutputs {
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        identifier_getter: &IG,
        address: Relocatable,
        constants: &HashMap<String, Felt>,
    ) -> VmUtilsResult<Relocatable> {
        let mut os_output_ptr = address;
        let mut contract_changes_writer = FullStateDiffWriter::new(vm);
        for output in &self.0 {
            os_output_ptr = write_full_os_output(
                output,
                vm,
                identifier_getter,
                os_output_ptr,
                constants,
                &mut contract_changes_writer,
            )?;
        }
        Ok(os_output_ptr)
    }
}
