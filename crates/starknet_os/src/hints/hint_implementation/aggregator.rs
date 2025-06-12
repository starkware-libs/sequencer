use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_from_var_name;

use crate::hints::error::OsHintResult;
use crate::hints::types::{HintArgs, HintArgsNoHP};
use crate::hints::vars::Ids;

#[allow(clippy::result_large_err)]
pub(crate) fn allocate_segments_for_messages(
    HintArgsNoHP { vm, ids_data, ap_tracking, .. }: HintArgsNoHP<'_>,
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
    HintArgs { hint_processor, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    hint_processor.serialize_data_availability_create_pages = false;
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn get_os_output_for_inner_blocks<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

#[allow(clippy::result_large_err)]
pub(crate) fn get_aggregator_output<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

#[allow(clippy::result_large_err)]
pub(crate) fn write_da_segment<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

#[allow(clippy::result_large_err)]
pub(crate) fn get_full_output_from_input<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

#[allow(clippy::result_large_err)]
pub(crate) fn get_use_kzg_da_from_input<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

#[allow(clippy::result_large_err)]
pub(crate) fn set_state_update_pointers_to_none<S: StateReader>(
    HintArgs { hint_processor, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    hint_processor.state_update_pointers = None;
    Ok(())
}
