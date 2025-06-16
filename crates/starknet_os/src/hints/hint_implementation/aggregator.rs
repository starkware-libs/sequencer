use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    insert_value_from_var_name,
    insert_value_into_ap,
};
use starknet_types_core::felt::Felt;

use crate::hint_processor::aggregator_hint_processor::AggregatorHintProcessor;
use crate::hint_processor::common_hint_processor::CommonHintProcessor;
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

pub(crate) fn disable_da_page_creation(
    hint_processor: &mut AggregatorHintProcessor<'_>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    hint_processor.serialize_data_availability_create_pages = false;
    Ok(())
}

pub(crate) fn get_os_output_for_inner_blocks(
    _hint_processor: &mut AggregatorHintProcessor<'_>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn get_aggregator_output(
    _hint_processor: &mut AggregatorHintProcessor<'_>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn write_da_segment(
    _hint_processor: &mut AggregatorHintProcessor<'_>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn get_full_output_from_input(
    hint_processor: &mut AggregatorHintProcessor<'_>,
    HintArgs { vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let full_output: Felt = hint_processor.input.full_output.into();
    insert_value_into_ap(vm, full_output)?;
    Ok(())
}

pub(crate) fn get_use_kzg_da_from_input(
    hint_processor: &mut AggregatorHintProcessor<'_>,
    HintArgs { vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let use_kzg_da: Felt = hint_processor.input.use_kzg_da.into();
    insert_value_into_ap(vm, use_kzg_da)?;
    Ok(())
}

pub(crate) fn set_state_update_pointers_to_none<'program, CHP: CommonHintProcessor<'program>>(
    hint_processor: &mut CHP,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    *hint_processor.get_mut_state_update_pointers() = None;
    Ok(())
}
