use std::collections::HashMap;

use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::vm_core::VirtualMachine;

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
