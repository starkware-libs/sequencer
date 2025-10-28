use std::collections::HashMap;

use apollo_starknet_os_program::OS_PROGRAM;
use cairo_vm::hint_processor::builtin_hint_processor::dict_hint_utils::DICT_ACCESS_SIZE;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::vm_core::VirtualMachine;
use itertools::Itertools;
use starknet_api::core::{ClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::hints::vars::CairoStruct;
use crate::io::os_output_types::{FullContractChanges, FullContractStorageUpdate};
use crate::vm_utils::get_address_of_nested_fields_from_base_address;

/// Creates a squashed dict from previous and new values, and stores it in a new memory segment.
pub fn allocate_squashed_cairo_dict(
    prev_values: &HashMap<Felt, MaybeRelocatable>,
    new_values: &HashMap<Felt, MaybeRelocatable>,
    vm: &mut VirtualMachine,
) -> (Relocatable, Relocatable) {
    let squashed_dict = flatten_cairo_dict(prev_values, new_values);
    let dict_segment_start = vm.add_memory_segment();
    let dict_segment_end = vm.load_data(dict_segment_start, &squashed_dict).unwrap();
    (dict_segment_start, dict_segment_end)
}

pub fn flatten_cairo_dict(
    prev_values: &HashMap<Felt, MaybeRelocatable>,
    new_values: &HashMap<Felt, MaybeRelocatable>,
) -> Vec<MaybeRelocatable> {
    let mut squashed_dict = vec![];
    let mut sorted_new_values: Vec<_> = new_values.iter().collect();
    sorted_new_values.sort_by_key(|(key, _)| *key);

    for (key, value) in sorted_new_values {
        let prev_value = prev_values.get(key).unwrap_or(&MaybeRelocatable::Int(Felt::ZERO));
        squashed_dict.push((*key).into());
        squashed_dict.push(prev_value.clone());
        squashed_dict.push(value.clone());
    }
    squashed_dict
}

pub fn parse_squashed_cairo_dict(squashed_dict: &[Felt]) -> HashMap<Felt, Felt> {
    assert!(squashed_dict.len() % DICT_ACCESS_SIZE == 0, "Invalid squashed dict length");
    let key_offset = 0;
    let new_val_offset = 2;
    squashed_dict
        .chunks(DICT_ACCESS_SIZE)
        .map(|chunk| (chunk[key_offset], chunk[new_val_offset]))
        .collect()
}

/// Parses a cairo dictionary from VM memory into a squashed dictionary.
/// Each entry is a tuple of the form (key, (prev_value, new_value)).
pub fn squash_dict(
    vm: &VirtualMachine,
    dict_start: Relocatable,
    dict_end: Relocatable,
) -> Vec<(Felt, (Felt, Felt))> {
    let mut prev_vals = HashMap::new();
    let mut new_vals = HashMap::new();
    let flat_dict: Vec<MaybeRelocatable> = vm
        .segments
        .memory
        .get_range(dict_start, (dict_end - dict_start).unwrap())
        .into_iter()
        .map(|item| item.unwrap().into_owned())
        .collect();
    for chunk in flat_dict.chunks_exact(DICT_ACCESS_SIZE) {
        let (key, prev, new) = (
            chunk.get(0).unwrap().get_int().unwrap(),
            chunk.get(1).unwrap().get_int().unwrap(),
            chunk.get(2).unwrap().get_int().unwrap(),
        );
        if !prev_vals.contains_key(&prev) {
            prev_vals.insert(key.clone(), prev);
        } else {
            assert_eq!(new_vals.get(&key).unwrap(), &prev);
        }
        new_vals.insert(key, new);
    }
    prev_vals
        .into_iter()
        .map(|(key, prev)| (key, (prev, new_vals.get(&key).unwrap().clone())))
        .collect()
}

/// Parses (from VM memory) a squashed cairo dictionary of contract changes.
/// Squashes the contract changes per contract address.
pub fn parse_contract_changes(
    vm: &VirtualMachine,
    dict_start: Relocatable,
    dict_end: Relocatable,
) -> HashMap<ContractAddress, FullContractChanges> {
    let flat_outer_dict: Vec<MaybeRelocatable> = vm
        .segments
        .memory
        .get_range(dict_start, (dict_end - dict_start).unwrap())
        .into_iter()
        .map(|item| item.unwrap().into_owned())
        .collect();
    assert!(flat_outer_dict.len() % DICT_ACCESS_SIZE == 0, "Invalid outer dict length");
    flat_outer_dict
        .chunks_exact(DICT_ACCESS_SIZE)
        .map(|chunk| {
            let (address, prev_state_entry_ptr, new_state_entry_ptr) = (
                ContractAddress(
                    PatriciaKey::try_from(chunk.get(0).unwrap().get_int().unwrap()).unwrap(),
                ),
                chunk.get(1).unwrap().get_relocatable().unwrap(),
                chunk.get(2).unwrap().get_relocatable().unwrap(),
            );

            // Fetch fields of previous and new state entries.
            // Note that nonces and class hash addresses point to integers, while storage pointer
            // points to a relocatable.
            let (
                prev_nonce_ptr,
                new_nonce_ptr,
                prev_class_hash_ptr,
                new_class_hash_ptr,
                prev_storage_ptr,
                new_storage_ptr,
            ) = [
                (prev_state_entry_ptr, "nonce"),
                (new_state_entry_ptr, "nonce"),
                (prev_state_entry_ptr, "class_hash"),
                (new_state_entry_ptr, "class_hash"),
                (prev_state_entry_ptr, "storage_ptr"),
                (new_state_entry_ptr, "storage_ptr"),
            ]
            .into_iter()
            .map(|(ptr, field)| {
                get_address_of_nested_fields_from_base_address(
                    ptr,
                    CairoStruct::StateEntry,
                    vm,
                    &[field],
                    &*OS_PROGRAM,
                )
                .unwrap()
            })
            .collect_tuple()
            .unwrap();

            let (prev_nonce, new_nonce, prev_class_hash, new_class_hash) =
                [prev_nonce_ptr, new_nonce_ptr, prev_class_hash_ptr, new_class_hash_ptr]
                    .into_iter()
                    .map(|ptr| vm.get_integer(ptr).unwrap())
                    .collect_tuple()
                    .unwrap();
            let (prev_storage_ptr, new_storage_ptr) = [prev_storage_ptr, new_storage_ptr]
                .into_iter()
                .map(|ptr| vm.get_relocatable(ptr).unwrap())
                .collect_tuple()
                .unwrap();

            let storage_changes = squash_dict(vm, prev_storage_ptr, new_storage_ptr)
                .into_iter()
                .map(|(key, (prev_value, new_value))| FullContractStorageUpdate {
                    key: StorageKey(PatriciaKey::try_from(key).unwrap()),
                    prev_value,
                    new_value,
                })
                .collect();

            (
                address,
                FullContractChanges {
                    addr: address,
                    prev_nonce: Nonce(*prev_nonce),
                    new_nonce: Nonce(*new_nonce),
                    prev_class_hash: ClassHash(*prev_class_hash),
                    new_class_hash: ClassHash(*new_class_hash),
                    storage_changes,
                },
            )
        })
        .collect()
}
