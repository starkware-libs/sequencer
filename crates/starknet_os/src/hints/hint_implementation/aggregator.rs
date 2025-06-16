use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    insert_value_from_var_name,
    insert_value_into_ap,
};
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::Ids;

#[allow(clippy::result_large_err)]
pub(crate) fn allocate_segments_for_messages(
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let segment1 = vm.add_temporary_segment();
    let segment2 = vm.add_temporary_segment();
    let initial_carried_outputs = vm.gen_arg(&vec![segment1, segment2])?;
    insert_value_from_var_name(
        Ids::InitialCarriedOutputs.into(),
        initial_carried_outputs,
        vm,
        ids_data,
        ap_tracking,
    )?;
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn disable_da_page_creation<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    hint_processor.serialize_data_availability_create_pages = false;
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn get_os_output_for_inner_blocks<S: StateReader>(
    _hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    todo!()
}

#[allow(clippy::result_large_err)]
pub(crate) fn get_aggregator_output<S: StateReader>(
    _hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    todo!()
}

#[allow(clippy::result_large_err)]
pub(crate) fn write_da_segment<S: StateReader>(
    _hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    todo!()
}

#[allow(clippy::result_large_err)]
pub(crate) fn get_full_output_from_input<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let full_output: Felt = hint_processor.os_hints_config.full_output.into();
    insert_value_into_ap(vm, full_output)?;
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn get_use_kzg_da_from_input<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let use_kzg_da: Felt = hint_processor.os_hints_config.use_kzg_da.into();
    insert_value_into_ap(vm, use_kzg_da)?;
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn set_state_update_pointers_to_none<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    hint_processor.state_update_pointers = None;
    Ok(())
}
