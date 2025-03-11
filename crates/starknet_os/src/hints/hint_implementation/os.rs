use std::collections::HashMap;

use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_into_ap;
use cairo_vm::types::relocatable::MaybeRelocatable;
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_types_core::felt::Felt;

use crate::hints::enum_definition::{AllHints, OsHint};
use crate::hints::error::OsHintResult;
use crate::hints::nondet_offsets::fetch_offset;
use crate::hints::types::HintArgs;

fn insert_hashmap_into_ap(vm: &mut VirtualMachine, hashmap: HashMap<Felt, Felt>) -> OsHintResult {
    let flattened_hashmap: Vec<MaybeRelocatable> = hashmap
        .iter()
        .flat_map(|entry| [MaybeRelocatable::from(entry.0), MaybeRelocatable::from(entry.1)])
        .collect();
    let _addr_after_hashmap = vm.load_data(vm.get_ap(), &flattened_hashmap)?;
    Ok(())
}

pub(crate) fn initialize_class_hashes<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let state_input = &hint_processor.execution_helper.cached_state;
    let class_hash_to_compiled_class_hash =
        &state_input.cache.clone().into_inner().initial_reads.compiled_class_hashes;
    insert_hashmap_into_ap(
        vm,
        class_hash_to_compiled_class_hash.iter().map(|entry| (entry.0.0, entry.1.0)).collect(),
    )
}

pub(crate) fn initialize_state_changes<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn write_full_output_to_memory<S: StateReader>(
    HintArgs { vm, hint_processor, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let os_input = &hint_processor.execution_helper.os_input;
    let full_output = Felt::from(os_input.full_output);
    let offset = fetch_offset(AllHints::OsHint(OsHint::WriteFullOutputToMemory))?;
    // TODO(Aner): maybe consider get_fp_with_offset(offset) instead of get_fp + offset? Or maybe
    // even insert_value_to_fp_with_offset?
    Ok(vm.insert_value((vm.get_fp() + offset)?, full_output)?)
}

pub(crate) fn configure_kzg_manager<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn set_ap_to_prev_block_hash<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let os_input = &hint_processor.execution_helper.os_input;
    Ok(insert_value_into_ap(vm, os_input.prev_block_hash.0)?)
}

pub(crate) fn set_ap_to_new_block_hash<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let os_input = &hint_processor.execution_helper.os_input;
    Ok(insert_value_into_ap(vm, os_input.new_block_hash.0)?)
}

pub(crate) fn starknet_os_input<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}
