use std::collections::HashMap;

use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_types_core::felt::Felt;

use crate::io::os_output::FullOsOutput;
use crate::vm_utils::{IdentifierGetter, LoadCairoObject, VmUtilsResult};

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
    _output: &FullOsOutput,
    _vm: &mut VirtualMachine,
    _identifier_getter: &IG,
    _address: Relocatable,
    _constants: &std::collections::HashMap<String, Felt>,
    _state_diff_writer: &mut FullStateDiffWriter,
) -> VmUtilsResult<Relocatable> {
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
