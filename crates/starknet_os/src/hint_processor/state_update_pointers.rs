use std::collections::HashMap;

use cairo_vm::types::relocatable::Relocatable;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::core::ContractAddress;

/// An equivalent to the `StateUpdatePointers` class in Python.
// TODO(Nimrod): Remove all `#[allow(dead_code)]` attributes after the code is fully implemented.
#[allow(dead_code)]
struct StateUpdatePointers {
    state_entries_ptr: Relocatable,
    classes_ptr: Relocatable,
    contract_address_to_state_entry_and_storage_ptr:
        HashMap<ContractAddress, (Relocatable, Relocatable)>,
}

impl StateUpdatePointers {
    #[allow(dead_code)]
    pub fn new(vm: &mut VirtualMachine) -> Self {
        Self {
            state_entries_ptr: vm.add_memory_segment(),
            classes_ptr: vm.add_memory_segment(),
            contract_address_to_state_entry_and_storage_ptr: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn get_contract_state_entry_and_storage_ptr(
        &mut self,
        contract_address: ContractAddress,
        vm: &mut VirtualMachine,
    ) -> (Relocatable, Relocatable) {
        *self
            .contract_address_to_state_entry_and_storage_ptr
            .entry(contract_address)
            .or_insert((vm.add_memory_segment(), vm.add_memory_segment()))
    }

    #[allow(dead_code)]
    pub fn set_contract_state_entry_and_storage_ptr(
        &mut self,
        contract_address: ContractAddress,
        state_entry_ptr: Relocatable,
        storage_ptr: Relocatable,
    ) {
        self.contract_address_to_state_entry_and_storage_ptr
            .insert(contract_address, (state_entry_ptr, storage_ptr));
    }

    #[allow(dead_code)]
    pub fn get_classes_ptr(&self) -> Relocatable {
        self.classes_ptr
    }

    #[allow(dead_code)]
    pub fn set_classes_ptr(&mut self, ptr: Relocatable) {
        self.classes_ptr = ptr;
    }

    #[allow(dead_code)]
    pub fn get_state_entries_ptr(&self) -> Relocatable {
        self.state_entries_ptr
    }

    #[allow(dead_code)]
    pub fn set_state_entries_ptr(&mut self, ptr: Relocatable) {
        self.state_entries_ptr = ptr;
    }
}
