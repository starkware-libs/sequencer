use std::collections::hash_map::Entry;
use std::collections::HashMap;

use cairo_vm::types::errors::math_errors::MathError;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::block::prev_block_number_into_felt;
use starknet_api::core::{ClassHash, Nonce};
use starknet_api::transaction::MessageToL1;
use starknet_types_core::felt::Felt;

use crate::hint_processor::state_update_pointers::{StateEntryPtr, StoragePtr};
use crate::io::os_output::{
    FullOsOutput,
    MessageToL2,
    MESSAGE_TO_L1_CONST_FIELD_SIZE,
    MESSAGE_TO_L2_CONST_FIELD_SIZE,
};
use crate::io::os_output_types::{
    FullCompiledClassHashUpdate,
    FullContractChanges,
    FullContractStorageUpdate,
};
use crate::vm_utils::{LoadIntoVmMemory, VmUtilsResult};

#[cfg(test)]
#[path = "aggregator_utils_test.rs"]
mod aggregator_utils_test;

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
    // A pointer to the end (first free memory cell) of an array of a specific contract's state
    // updates: a triplet of the form (class_hash, storage_dict_ptr, nonce), where storage_dict_ptr
    // is defined below.
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
    // A pointer to the end (first free memory cell) of a cairo dict for state updates, in the form
    // of {contract address: (prev_state, new_state)}. A state is a triplet represented by the
    // StateEntry cairo struct.
    state_dict_ptr: Relocatable,
    // A pointer to the end (first free memory cell) of a (cairo) dict for class hash updates:
    // {class_hash: (prev_compiled_class_hash, new_compiled_class_hash)}.
    class_dict_ptr: Relocatable,
    // A dict from class hash to a representation dict: from storage key to StateEntry (class_hash,
    // storage_dict_ptr, nonce).
    inner_storage: HashMap<MaybeRelocatable, StateEntryManager>,
}

impl FullStateDiffWriter {
    pub(crate) fn new(vm: &mut VirtualMachine) -> Self {
        Self {
            state_dict_ptr: vm.add_memory_segment(),
            class_dict_ptr: vm.add_memory_segment(),
            inner_storage: HashMap::new(),
        }
    }

    pub(crate) fn get_state_dict_ptr(&self) -> Relocatable {
        self.state_dict_ptr
    }

    pub(crate) fn get_class_dict_ptr(&self) -> Relocatable {
        self.class_dict_ptr
    }

    pub(crate) fn write_contract_changes(
        &mut self,
        contracts: &[FullContractChanges],
        vm: &mut VirtualMachine,
    ) -> VmUtilsResult<()> {
        let mut state_dict = Vec::with_capacity(contracts.len() * 3);
        for contract in contracts {
            let contract_address: MaybeRelocatable = (**contract.addr).into();
            // Exists in inner_storage if this contract was changed in a previous state diff.
            let state_manager = match self.inner_storage.entry(contract_address.clone()) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => {
                    // Write the initial `StateEntry` struct (the prev values in the first state
                    // diff the contract was changed in) into memory.
                    entry.insert(StateEntryManager::new_state_entry(
                        vm,
                        contract.prev_class_hash,
                        contract.prev_nonce,
                    )?)
                }
            };

            state_manager.add_state_entry(
                vm,
                contract.new_class_hash,
                &contract.storage_changes,
                contract.new_nonce,
            )?;

            state_dict.push(contract_address);
            state_dict.push((state_manager.get_prev_state_entry_ptr()?).0.into());
            state_dict.push(state_manager.get_last_state_entry_ptr().0.into())
        }
        self.state_dict_ptr = vm.load_data(self.state_dict_ptr, &state_dict)?;
        Ok(())
    }

    pub(crate) fn write_classes_changes(
        &mut self,
        classes: &Vec<FullCompiledClassHashUpdate>,
        vm: &mut VirtualMachine,
    ) -> VmUtilsResult<()> {
        self.class_dict_ptr = classes.load_into_vm_memory(vm, self.class_dict_ptr)?;
        Ok(())
    }
}

/// Writes the given `FullOsOutput` to the VM at the specified address.
fn write_full_os_output(
    output: &FullOsOutput,
    vm: &mut VirtualMachine,
    address: Relocatable,
    state_diff_writer: &mut FullStateDiffWriter,
) -> VmUtilsResult<Relocatable> {
    let FullOsOutput { common_os_output, state_diff } = output;
    let messages_to_l1_start = vm.add_temporary_segment();
    let messages_to_l1_end =
        common_os_output.messages_to_l1.load_into_vm_memory(vm, messages_to_l1_start)?;

    let messages_to_l2_start = vm.add_temporary_segment();
    let messages_to_l2_end =
        common_os_output.messages_to_l2.load_into_vm_memory(vm, messages_to_l2_start)?;
    // A cairo dict from contract address to StateEntry (class_hash, storage_dict_ptr, nonce).
    // See StateEntryManager::storage_dict_ptr for an explanation about storage_dict_ptr.
    let state_dict_ptr_start = state_diff_writer.get_state_dict_ptr();
    state_diff_writer.write_contract_changes(&state_diff.contracts, vm)?;

    let class_dict_ptr_start = state_diff_writer.get_class_dict_ptr();
    state_diff_writer.write_classes_changes(&state_diff.classes, vm)?;

    // Write the OsOutput struct into cairo.
    let state_update_output = vm.gen_arg(&vec![
        MaybeRelocatable::Int(common_os_output.initial_root),
        common_os_output.final_root.into(),
    ])?;

    let header = vm.gen_arg(&vec![
        state_update_output,
        prev_block_number_into_felt(common_os_output.prev_block_number).into(),
        Felt::from(common_os_output.new_block_number.0).into(),
        common_os_output.prev_block_hash.into(),
        common_os_output.new_block_hash.into(),
        common_os_output.os_program_hash.into(),
        common_os_output.starknet_os_config_hash.into(),
        Felt::ZERO.into(), // use_kzg_da field (False in the aggregator input).
        Felt::ONE.into(),  // full_output field (True in the aggregator input).
    ])?;

    let squashed_os_state_update = vm.gen_arg(&vec![
        MaybeRelocatable::RelocatableValue(state_dict_ptr_start),
        Felt::from(state_diff.contracts.len()).into(),
        MaybeRelocatable::RelocatableValue(class_dict_ptr_start),
        Felt::from(state_diff.classes.len()).into(),
    ])?;

    let initial_carried_outputs = vm.gen_arg(&vec![messages_to_l1_start, messages_to_l2_start])?;
    let final_carried_outputs = vm.gen_arg(&vec![messages_to_l1_end, messages_to_l2_end])?;

    Ok(vm.load_data(
        address,
        &[header, squashed_os_state_update, initial_carried_outputs, final_carried_outputs],
    )?)
}

pub(crate) struct FullOsOutputs(pub Vec<FullOsOutput>);

impl LoadIntoVmMemory for FullOsOutputs {
    fn load_into_vm_memory(
        &self,
        vm: &mut VirtualMachine,
        address: Relocatable,
    ) -> VmUtilsResult<Relocatable> {
        let mut os_output_ptr = address;
        let mut contract_changes_writer = FullStateDiffWriter::new(vm);
        for output in &self.0 {
            os_output_ptr =
                write_full_os_output(output, vm, os_output_ptr, &mut contract_changes_writer)?;
        }
        Ok(os_output_ptr)
    }
}
