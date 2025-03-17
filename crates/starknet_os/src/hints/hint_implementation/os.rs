use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_into_ap;
use starknet_types_core::felt::Felt;

use crate::hints::enum_definition::{AllHints, OsHint};
use crate::hints::error::OsHintResult;
use crate::hints::nondet_offsets::insert_nondet_hint_value;
use crate::hints::types::HintArgs;
use crate::hints::vars::Scope;
use crate::io::os_input::OsBlockInput;

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
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn write_full_output_to_memory<S: StateReader>(
    HintArgs { vm, exec_scopes, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let block_input: &OsBlockInput = exec_scopes.get(Scope::BlockInput.into())?;
    // TODO(Meshi): when it will be a part of multi-block input take it from the os_input
    let full_output = Felt::from(block_input.full_output);
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
    HintArgs { exec_scopes, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let block_input: &OsBlockInput = exec_scopes.get(Scope::BlockInput.into())?;
    Ok(insert_value_into_ap(vm, block_input.prev_block_hash.0)?)
}

pub(crate) fn set_ap_to_new_block_hash<S: StateReader>(
    HintArgs { exec_scopes, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let block_input: &OsBlockInput = exec_scopes.get(Scope::BlockInput.into())?;
    Ok(insert_value_into_ap(vm, block_input.new_block_hash.0)?)
}

pub(crate) fn starknet_os_input<S: StateReader>(
    HintArgs { exec_scopes, hint_processor, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    // Nothing to do here; OS input already available on the hint processor.
    let block_input_iter =
        hint_processor.execution_helper.os_input.blocks_inputs.clone().into_iter();
    exec_scopes.insert_value(Scope::BlockInputIter.into(), block_input_iter);
    Ok(())
}

pub(crate) fn init_state_update_pointer<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn get_n_blocks<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn create_block_additional_hints<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}
