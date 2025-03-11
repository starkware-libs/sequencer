use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_into_ap;
use starknet_types_core::felt::Felt;

use crate::hints::enum_definition::{AllHints, OsHint};
use crate::hints::error::OsHintResult;
use crate::hints::nondet_offsets::insert_nondet_hint_value;
use crate::hints::types::HintArgs;
use crate::hints::vars::Scope;

// pub const INITIALIZE_CLASS_HASHES: &str = "initial_dict =
// os_input.class_hash_to_compiled_class_hash";

// pub fn initialize_class_hashes(
//     _vm: &mut VirtualMachine,
//     exec_scopes: &mut ExecutionScopes,
//     _ids_data: &HashMap<String, HintReference>,
//     _ap_tracking: &ApTracking,
//     _constants: &HashMap<String, Felt252>,
// ) -> Result<(), HintError> { let os_input =
//   exec_scopes.get::<Rc<StarknetOsInput>>(vars::scopes::OS_INPUT)?; let mut class_dict:
//   HashMap<MaybeRelocatable, MaybeRelocatable> = HashMap::new(); for (class_hash,
//   compiled_class_hash) in &os_input.class_hash_to_compiled_class_hash {
//   class_dict.insert(MaybeRelocatable::from(class_hash),
//   MaybeRelocatable::from(compiled_class_hash)); }

//     exec_scopes.insert_box(vars::scopes::INITIAL_DICT, Box::new(class_dict));
//     Ok(())
// }

pub(crate) fn initialize_class_hashes<S: StateReader>(
    HintArgs { hint_processor, exec_scopes, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let state_input = &hint_processor.execution_helper.cached_state;
    let class_hash_to_compiled_class_hash =
        state_input.cache.clone().into_inner().initial_reads.compiled_class_hashes;
    // TODO(Aner): verify that inserting the hashmap to the scope directly is possible.
    exec_scopes.insert_value(Scope::InitialDict.into(), class_hash_to_compiled_class_hash);
    Ok(())
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
