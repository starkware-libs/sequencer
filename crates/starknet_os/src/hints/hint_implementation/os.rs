use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_into_ap;

use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;
use crate::io::os_input;

pub(crate) fn initialize_class_hashes<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn initialize_state_changes<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn write_full_output_to_memory<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn configure_kzg_manager<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

// pub const SET_AP_TO_PREV_BLOCK_HASH: &str =
//     indoc! {r#"memory[ap] = to_felt_or_relocatable(os_input.prev_block_hash)"#};

// pub fn set_ap_to_prev_block_hash(
//     vm: &mut VirtualMachine,
//     exec_scopes: &mut ExecutionScopes,
//     _ids_data: &HashMap<String, HintReference>,
//     _ap_tracking: &ApTracking,
//     _constants: &HashMap<String, Felt252>,
// ) -> Result<(), HintError> { let os_input: Rc<StarknetOsInput> =
//   exec_scopes.get(vars::scopes::OS_INPUT)?; insert_value_into_ap(vm, os_input.prev_block_hash)?;

//     Ok(())
// }

pub(crate) fn set_ap_to_prev_block_hash<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let os_input = &hint_processor.execution_helper.os_input;
    Ok(insert_value_into_ap(vm, os_input.prev_block_hash.0)?)
}

// pub const SET_AP_TO_NEW_BLOCK_HASH: &str =
//     "memory[ap] = to_felt_or_relocatable(os_input.new_block_hash)";

// pub fn set_ap_to_new_block_hash(
//     vm: &mut VirtualMachine,
//     exec_scopes: &mut ExecutionScopes,
//     _ids_data: &HashMap<String, HintReference>,
//     _ap_tracking: &ApTracking,
//     _constants: &HashMap<String, Felt252>,
// ) -> Result<(), HintError> { let os_input: Rc<StarknetOsInput> =
//   exec_scopes.get(vars::scopes::OS_INPUT)?; insert_value_into_ap(vm, os_input.new_block_hash)?;

//     Ok(())
// }

pub(crate) fn set_ap_to_new_block_hash<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let os_input = &hint_processor.execution_helper.os_input;
    Ok(insert_value_into_ap(vm, os_input.new_block_hash.0)?)
}

pub(crate) fn starknet_os_input<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}
