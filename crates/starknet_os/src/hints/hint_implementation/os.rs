use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_into_ap;
use starknet_types_core::felt::Felt;

use crate::hints::enum_definition::{AllHints, OsHint};
use crate::hints::error::OsHintResult;
use crate::hints::nondet_offsets::fetch_offset;
use crate::hints::types::HintArgs;

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

// pub const WRITE_FULL_OUTPUT_TO_MEM: &str = indoc! {r#"memory[fp + 19] =
// to_felt_or_relocatable(os_input.full_output)"#};

// pub fn write_full_output_to_mem(
//     vm: &mut VirtualMachine,
//     exec_scopes: &mut ExecutionScopes,
//     _ids_data: &HashMap<String, HintReference>,
//     _ap_tracking: &ApTracking,
//     _constants: &HashMap<String, Felt252>,
// ) -> Result<(), HintError> { let os_input: Rc<StarknetOsInput> =
//   exec_scopes.get(vars::scopes::OS_INPUT)?; let full_output = os_input.full_output;

//     vm.insert_value((vm.get_fp() + 19)?, Felt252::from(full_output)).map_err(HintError::Memory)
// }

pub(crate) fn write_full_output_to_memory<S: StateReader>(
    HintArgs { vm, hint_processor, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let os_input = &hint_processor.execution_helper.os_input;
    let full_output = Felt::from(os_input.full_output);
    // TODO(Aner): the offsets don't match - here it's 16, in Moonsong it's 19.
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
