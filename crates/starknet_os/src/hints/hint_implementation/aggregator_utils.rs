use std::collections::HashMap;

use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::transaction::MessageToL1;
use starknet_types_core::felt::Felt;

use crate::io::os_output::{
    MessageToL2,
    MESSAGE_TO_L1_CONST_FIELD_SIZE,
    MESSAGE_TO_L2_CONST_FIELD_SIZE,
};
use crate::vm_utils::{LoadIntoVmMemory, VmUtilsResult};

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

impl<T: ToMaybeRelocatables> LoadIntoVmMemory for T {
    fn load_into_vm_memory(
        &self,
        vm: &mut VirtualMachine,
        address: Relocatable,
    ) -> VmUtilsResult<Relocatable> {
        Ok(vm.load_data(address, &self.to_maybe_relocatables())?)
    }
}

struct StateEntry {
    class_hash: ClassHash,
    storage_dict_ptr: StoragePtr,
    nonce: Nonce,
}

impl StateEntry {
    fn size() -> usize {
        3
    }
}

impl ToMaybeRelocatables for StateEntry {
    fn to_maybe_relocatables(&self) -> Vec<MaybeRelocatable> {
        vec![self.class_hash.0.into(), self.storage_dict_ptr.0.into(), self.nonce.0.into()]
    }
}

/// A helper struct to manage the state updates tracking for a single contract over multiple
/// state diffs (i.e., in multiple OsOutputs).
struct StateEntryManager {
    // A pointer to the end (first free memory cell) of an array of state updates: a triplet of
    // the form (class_hash, storage_dict_ptr, nonce), where storage_dict_ptr is defined below.
    state_entry_ptr: StateEntryPtr,
    // A pointer to the end (first free memory cell) of a specific contract's storage updates
    // cairo dict of the form {storage_key: (prev_value, new_value)}.
    storage_dict_ptr: StoragePtr,
}

impl StateEntryManager {
    /// Writes the initial `StateEntry` struct.
    fn new_state_entry(
        vm: &mut VirtualMachine,
        class_hash: ClassHash,
        nonce: Nonce,
    ) -> VmUtilsResult<Self> {
        let mut new_manager = Self {
            state_entry_ptr: StateEntryPtr(vm.add_memory_segment()),
            storage_dict_ptr: StoragePtr(vm.add_memory_segment()),
        };
        new_manager.add_state_entry(vm, class_hash, &vec![], nonce)?;
        Ok(new_manager)
    }

    /// Writes a new `StateEntry` update instance to the memory.
    fn add_state_entry(
        &mut self,
        vm: &mut VirtualMachine,
        class_hash: ClassHash,
        storage_changes: &Vec<FullContractStorageUpdate>,
        nonce: Nonce,
    ) -> VmUtilsResult<()> {
        self.storage_dict_ptr =
            StoragePtr(storage_changes.load_into_vm_memory(vm, self.storage_dict_ptr.0)?);

        let state_entry = StateEntry { class_hash, storage_dict_ptr: self.storage_dict_ptr, nonce };
        self.state_entry_ptr =
            StateEntryPtr(state_entry.load_into_vm_memory(vm, self.state_entry_ptr.0)?);
        Ok(())
    }

    /// Returns the pointer to the start of the previous written `StateEntry` struct.
    /// Valid only if a call to add_state_entry was made before.
    fn get_prev_state_entry_ptr(&self) -> Result<StateEntryPtr, MathError> {
        Ok(StateEntryPtr((self.state_entry_ptr.0 - 2 * StateEntry::size())?))
    }

    /// Returns the pointer to the start of the last written `StateEntry` struct.
    fn get_last_state_entry_ptr(&self) -> StateEntryPtr {
        StateEntryPtr((self.state_entry_ptr.0 - StateEntry::size()).unwrap_or_else(|_| {
            panic!(
                "Unexpected StateEntryPtr underflow from StateEntryManager. Ptr=: {}",
                self.state_entry_ptr.0
            )
        }))
    }
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
