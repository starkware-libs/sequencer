use blockifier::state::state_api::StateReader;
use cairo_vm::any_box;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_into_ap;
use cairo_vm::types::exec_scope::ExecutionScopes;
use starknet_types_core::felt::Felt;

use crate::hints::enum_definition::{AllHints, OsHint};
use crate::hints::error::OsHintResult;
use crate::hints::nondet_offsets::insert_nondet_hint_value;
use crate::hints::types::HintArgs;
use crate::hints::vars::Scope;

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
    HintArgs { vm, hint_processor, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let os_input = &hint_processor.execution_helper.os_input;
    let full_output = Felt::from(os_input.full_output);
    insert_nondet_hint_value(vm, AllHints::OsHint(OsHint::WriteFullOutputToMemory), full_output)
}

// pub const CONFIGURE_KZG_MANAGER: &str = indoc! {r#"__serialize_data_availability_create_pages__ =
// True kzg_manager = execution_helper.kzg_manager"#};

// pub fn configure_kzg_manager(
//     vm: &mut VirtualMachine,
//     exec_scopes: &mut ExecutionScopes,
//     ids_data: &HashMap<String, HintReference>,
//     ap_tracking: &ApTracking,
//     constants: &HashMap<String, Felt252>,
// ) -> Result<(), HintError> { execute_coroutine(configure_kzg_manager_async(vm, exec_scopes,
//   ids_data, ap_tracking, constants))?
// }
// pub async fn configure_kzg_manager_async(
//     _vm: &mut VirtualMachine,
//     exec_scopes: &mut ExecutionScopes,
//     _ids_data: &HashMap<String, HintReference>,
//     _ap_tracking: &ApTracking,
//     _constants: &HashMap<String, Felt252>,
// ) -> Result<(), HintError> { set_variable_in_root_exec_scope(exec_scopes,
//   vars::scopes::SERIALIZE_DATA_AVAILABILITY_CREATE_PAGES, true);

//     // We don't leave kzg_manager in scope here, it can be obtained through execution_helper
// later

//     Ok(())
// }

pub fn insert_value_to_root_scope<T: 'static>(
    exec_scopes: &mut ExecutionScopes,
    name: &str,
    value: T,
) {
    exec_scopes.data[0].insert(name.to_string(), any_box!(value));
}

pub(crate) fn configure_kzg_manager<S: StateReader>(
    HintArgs { exec_scopes, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    insert_value_to_root_scope(
        exec_scopes,
        Scope::SerializeDataAvailabilityCreatePages.into(),
        true,
    );
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
    todo!()
}
