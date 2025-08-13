use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    insert_value_from_var_name,
    insert_value_into_ap,
};
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_types_core::felt::Felt;

use crate::hint_processor::aggregator_hint_processor::{AggregatorHintProcessor, DataAvailability};
use crate::hint_processor::common_hint_processor::CommonHintProcessor;
use crate::hints::error::OsHintResult;
use crate::hints::hint_implementation::output::load_public_keys_into_memory;
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
    hint_processor: &mut AggregatorHintProcessor<'_>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    // This impl differes from the python one, as we don't need to support an input of
    // polynomial_coefficients_to_kzg_commitment function anymore.
    hint_processor.serialize_data_availability_create_pages = true;
    Ok(())
}

pub(crate) fn write_da_segment(
    hint_processor: &mut AggregatorHintProcessor<'_>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    if let DataAvailability::Blob(da_file_path) = hint_processor.input.da.clone() {
        let da_segment = hint_processor.get_da_segment();

        std::fs::write(da_file_path, serde_json::to_string(&da_segment)?)?;
    }
    Ok(())
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
    let use_kzg_da: Felt = match hint_processor.input.da {
        DataAvailability::Blob(_) => true,
        DataAvailability::CallData => false,
    }
    .into();
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

pub(crate) fn get_chain_id_from_input(
    hint_processor: &mut AggregatorHintProcessor<'_>,
    HintArgs { vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let chain_id: Felt = hint_processor.input.chain_id;
    insert_value_into_ap(vm, chain_id)?;
    Ok(())
}

pub(crate) fn get_fee_token_address_from_input(
    hint_processor: &mut AggregatorHintProcessor<'_>,
    HintArgs { vm, .. }: HintArgs<'_>,
) -> OsHintResult {
    let fee_token_address: Felt = hint_processor.input.fee_token_address;
    insert_value_into_ap(vm, fee_token_address)?;
    Ok(())
}

pub(crate) fn get_public_keys_from_aggregator_input(
    hint_processor: &mut AggregatorHintProcessor<'_>,
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let public_keys = &hint_processor.input.public_keys;
    load_public_keys_into_memory(vm, ids_data, ap_tracking, public_keys.clone())?;
    Ok(())
}
