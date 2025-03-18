use std::collections::HashMap;

use blockifier::state::cached_state::StateMaps;
use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_into_ap;
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::enum_definition::{AllHints, OsHint};
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::nondet_offsets::insert_nondet_hint_value;
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Scope};
use crate::vm_utils::insert_values_to_fields;

fn get_state_maps<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<S>,
) -> Result<StateMaps, OsHintError> {
    let state_maps = hint_processor.execution_helper.cached_state.get_state_maps()?;
    Ok(state_maps)
}

pub(crate) fn initialize_class_hashes<S: StateReader>(
    HintArgs { hint_processor, exec_scopes, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let state_maps = get_state_maps(hint_processor)?;
    let class_hash_to_compiled_class_hash = state_maps.compiled_class_hashes;
    exec_scopes.insert_value(Scope::InitialDict.into(), class_hash_to_compiled_class_hash);
    Ok(())
}

pub(crate) fn initialize_state_changes<S: StateReader>(
    HintArgs { hint_processor, exec_scopes, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let state_maps = get_state_maps(hint_processor)?;
    let mut initial_dict: HashMap<MaybeRelocatable, MaybeRelocatable> = HashMap::new();
    for (address, nonce) in state_maps.nonces.iter() {
        let class_hash = state_maps.class_hashes.get(address).expect("Missing contract hash");
        let state_entry_base = vm.add_memory_segment();
        let storage_ptr = vm.add_memory_segment();
        insert_values_to_fields(
            state_entry_base,
            CairoStruct::StateEntry,
            vm,
            &[
                ("class_hash".to_string(), MaybeRelocatable::from(**class_hash)),
                ("storage_ptr".to_string(), storage_ptr.into()),
                ("nonce".to_string(), MaybeRelocatable::from(**nonce)),
            ],
            &hint_processor.execution_helper.os_program,
        )?;
        initial_dict.insert(address.0.0.into(), state_entry_base.into());
    }
    exec_scopes.insert_value(Scope::InitialDict.into(), initial_dict);
    Ok(())
}

pub(crate) fn write_full_output_to_memory<S: StateReader>(
    HintArgs { vm, hint_processor, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let os_input = &hint_processor.execution_helper.os_input;
    let full_output = Felt::from(os_input.full_output);
    insert_nondet_hint_value(vm, AllHints::OsHint(OsHint::WriteFullOutputToMemory), full_output)
}

pub(crate) fn configure_kzg_manager<S: StateReader>(
    HintArgs { exec_scopes, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    // TODO(Aner): verify that inserting into the "root" scope is not neccessary.
    exec_scopes.insert_value(Scope::SerializeDataAvailabilityCreatePages.into(), true);
    Ok(())
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
    // Nothing to do here; OS input already available on the hint processor.
    Ok(())
}

pub(crate) fn init_state_update_pointer<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}
