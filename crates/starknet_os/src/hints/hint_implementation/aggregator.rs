use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::insert_value_from_var_name;

use crate::hint_processor::aggregator_hint_processor::AggregatorHintProcessor;
use crate::hints::error::OsHintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::Ids;

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
pub(crate) fn disable_da_page_creation(
    hint_processor: &mut AggregatorHintProcessor<'_>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    hint_processor.serialize_data_availability_create_pages = false;
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn get_os_output_for_inner_blocks(
    _hint_processor: &mut AggregatorHintProcessor<'_>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    todo!()
}

#[allow(clippy::result_large_err)]
pub(crate) fn get_aggregator_output(
    _hint_processor: &mut AggregatorHintProcessor<'_>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    todo!()
}

#[allow(clippy::result_large_err)]
pub(crate) fn write_da_segment(
    _hint_processor: &mut AggregatorHintProcessor<'_>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    todo!()
}

#[allow(clippy::result_large_err)]
pub(crate) fn get_full_output_from_input(
    _hint_processor: &mut AggregatorHintProcessor<'_>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    todo!()
}

#[allow(clippy::result_large_err)]
pub(crate) fn get_use_kzg_da_from_input(
    _hint_processor: &mut AggregatorHintProcessor<'_>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    todo!()
}

#[allow(clippy::result_large_err)]
pub(crate) fn set_state_update_pointers_to_none(
    hint_processor: &mut AggregatorHintProcessor<'_>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    hint_processor.state_update_pointers = None;
    Ok(())
}
