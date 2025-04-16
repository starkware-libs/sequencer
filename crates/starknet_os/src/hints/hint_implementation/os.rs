use std::collections::HashMap;

use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_into_ap;
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_types_core::felt::Felt;

use crate::hint_processor::state_update_pointers::StateUpdatePointers;
use crate::hints::enum_definition::{AllHints, OsHint};
use crate::hints::error::OsHintResult;
use crate::hints::nondet_offsets::insert_nondet_hint_value;
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Scope};
use crate::vm_utils::insert_values_to_fields;

pub(crate) fn initialize_class_hashes<S: StateReader>(
    HintArgs { hint_processor, exec_scopes, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let class_hash_to_compiled_class_hash: HashMap<MaybeRelocatable, MaybeRelocatable> =
        hint_processor
            .get_current_execution_helper()?
            .cached_state
            .writes_compiled_class_hashes()
            .into_iter()
            .map(|(class_hash, compiled_class_hash)| {
                (class_hash.0.into(), compiled_class_hash.0.into())
            })
            .collect();
    exec_scopes.insert_value(Scope::InitialDict.into(), class_hash_to_compiled_class_hash);
    Ok(())
}

pub(crate) fn initialize_state_changes<S: StateReader>(
    HintArgs { hint_processor, exec_scopes, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let cached_state = &hint_processor.get_current_execution_helper()?.cached_state;
    let writes_accessed_addresses = cached_state.writes_contract_addresses();
    let mut initial_dict: HashMap<MaybeRelocatable, MaybeRelocatable> = HashMap::new();

    for contract_address in writes_accessed_addresses {
        let nonce = cached_state.get_nonce_at(contract_address)?;
        let class_hash = cached_state.get_class_hash_at(contract_address)?;
        let state_entry_base = vm.add_memory_segment();
        let storage_ptr = vm.add_memory_segment();
        insert_values_to_fields(
            state_entry_base,
            CairoStruct::StateEntry,
            vm,
            &[
                ("class_hash", class_hash.0.into()),
                ("storage_ptr", storage_ptr.into()),
                ("nonce", nonce.0.into()),
            ],
            &hint_processor.os_program,
        )?;
        initial_dict.insert((*contract_address.0.key()).into(), state_entry_base.into());
    }
    exec_scopes.insert_value(Scope::InitialDict.into(), initial_dict);
    Ok(())
}

pub(crate) fn write_full_output_to_memory<S: StateReader>(
    HintArgs { vm, hint_processor, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let full_output = Felt::from(hint_processor.os_hints_config.full_output);
    insert_nondet_hint_value(vm, AllHints::OsHint(OsHint::WriteFullOutputToMemory), full_output)
}

pub(crate) fn configure_kzg_manager<S: StateReader>(
    HintArgs { hint_processor, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    hint_processor.serialize_data_availability_create_pages = true;
    Ok(())
}

pub(crate) fn set_ap_to_prev_block_hash<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let os_input = &hint_processor.get_current_execution_helper()?.os_block_input;
    Ok(insert_value_into_ap(vm, os_input.prev_block_hash.0)?)
}

pub(crate) fn set_ap_to_new_block_hash<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let os_input = &hint_processor.get_current_execution_helper()?.os_block_input;
    Ok(insert_value_into_ap(vm, os_input.new_block_hash.0)?)
}

pub(crate) fn starknet_os_input<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    // Nothing to do here; OS input already available on the hint processor.
    Ok(())
}

pub(crate) fn init_state_update_pointer<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    hint_processor.state_update_pointers = Some(StateUpdatePointers::new(vm));
    Ok(())
}

pub(crate) fn get_n_blocks<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let n_blocks = hint_processor.n_blocks();
    insert_nondet_hint_value(vm, AllHints::OsHint(OsHint::GetBlocksNumber), n_blocks)
}

pub(crate) fn create_block_additional_hints<S: StateReader>(
    HintArgs { hint_processor, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    // TODO(Nimrod): Verify hint implementation once syscall handlers are per block.
    hint_processor.execution_helpers_manager.increment_current_helper_index();
    Ok(())
}
