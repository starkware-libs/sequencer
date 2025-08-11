use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_ptr_from_var_name,
    insert_value_from_var_name,
    insert_value_into_ap,
};
use cairo_vm::hint_processor::hint_processor_utils::felt_to_usize;
use starknet_types_core::felt::Felt;

use crate::hint_processor::aggregator_hint_processor::{AggregatorHintProcessor, DataAvailability};
use crate::hint_processor::common_hint_processor::CommonHintProcessor;
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::hint_implementation::aggregator_utils::FullOsOutputs;
use crate::hints::types::HintArgs;
use crate::hints::vars::Ids;
use crate::io::os_output::{wrap_missing, FullOsOutput, OsOutput};
use crate::io::os_output_types::TryFromOutputIter;
use crate::vm_utils::LoadIntoVmMemory;

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
    hint_processor: &mut AggregatorHintProcessor<'_>,
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let mut bootloader_iter = hint_processor
        .input
        .bootloader_output
        .take()
        .ok_or(OsHintError::AggregatorBootloaderInputAlreadyConsumed)?
        .into_iter();
    let n_outputs = wrap_missing(bootloader_iter.next(), "n_output")?;
    let n_outputs_usize = felt_to_usize(&n_outputs)?;
    assert!(n_outputs_usize > 0, "No tasks found in the bootloader output.");

    let mut program_hash: Option<Felt> = None;
    let mut outputs = FullOsOutputs(Vec::<FullOsOutput>::with_capacity(n_outputs_usize));
    for _ in 0..n_outputs_usize {
        wrap_missing(bootloader_iter.next(), "output_size")?;
        let current_output_program_hash = wrap_missing(bootloader_iter.next(), "program_hash")?;
        assert_eq!(
            program_hash.get_or_insert(current_output_program_hash),
            &current_output_program_hash
        );
        outputs.0.push(OsOutput::try_from_output_iter(&mut bootloader_iter)?.try_into()?);
    }

    insert_value_from_var_name(
        Ids::OsProgramHash.into(),
        program_hash.expect("n_outputs > 0 but program hash wasn't initialized."),
        vm,
        ids_data,
        ap_tracking,
    )?;

    insert_value_from_var_name(Ids::NTasks.into(), n_outputs, vm, ids_data, ap_tracking)?;

    let os_output_ptr = get_ptr_from_var_name(Ids::OsOutputs.into(), vm, ids_data, ap_tracking)?;
    outputs.load_into_vm_memory(vm, os_output_ptr)?;
    Ok(())
}

pub(crate) fn get_aggregator_output(
    hint_processor: &mut AggregatorHintProcessor<'_>,
    HintArgs { .. }: HintArgs<'_>,
) -> OsHintResult {
    // This impl differs from the python one, as we don't need to support an input of
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
