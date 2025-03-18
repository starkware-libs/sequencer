use std::collections::HashMap;

use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_into_ap;
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_types_core::felt::Felt;

use crate::hints::enum_definition::{AllHints, OsHint};
use crate::hints::error::OsHintResult;
use crate::hints::nondet_offsets::insert_nondet_hint_value;
use crate::hints::types::HintArgs;
use crate::hints::vars::Scope;

pub(crate) fn initialize_class_hashes<S: StateReader>(
    HintArgs { hint_processor, exec_scopes, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let state_input = &hint_processor.execution_helper.cached_state;
    let class_hash_to_compiled_class_hash =
        state_input.cache.clone().into_inner().initial_reads.compiled_class_hashes;
    exec_scopes.insert_value(Scope::InitialDict.into(), class_hash_to_compiled_class_hash);
    Ok(())
}

pub(crate) fn initialize_state_changes<S: StateReader>(
    HintArgs { hint_processor, exec_scopes, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let contract_items = &hint_processor.execution_helper.cached_state.get_initial_reads()?;
    let mut initial_dict: HashMap<MaybeRelocatable, MaybeRelocatable> = HashMap::new();
    for (address, nonce) in contract_items.nonces.iter() {
        let contract_hash =
            contract_items.class_hashes.get(address).expect("Missing contract hash");
        let change_base = vm.add_memory_segment();
        vm.insert_value(change_base, **contract_hash)?;
        let new_segment = vm.add_memory_segment();
        vm.insert_value((change_base + 1)?, new_segment)?;
        vm.insert_value((change_base + 2)?, **nonce)?;

        initial_dict.insert(address.0.0.into(), change_base.into());
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
