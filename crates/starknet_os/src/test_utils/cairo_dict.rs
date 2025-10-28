use std::collections::HashMap;

use cairo_vm::hint_processor::builtin_hint_processor::dict_hint_utils::DICT_ACCESS_SIZE;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_types_core::felt::Felt;

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
