use std::cmp::min;

use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
    insert_value_from_var_name,
};
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::types::{HintArgs, HintArgsNoHP};
use crate::hints::vars::{Const, Ids, Scope};

const MAX_PAGE_SIZE: usize = 3800;
const OUTPUT_ATTRIBUTE_FACT_TOPOLOGY: &str = "gps_fact_topology";

#[allow(clippy::result_large_err)]
pub(crate) fn set_tree_structure<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    if !hint_processor.serialize_data_availability_create_pages {
        return Ok(());
    }
    let onchain_data_start = get_ptr_from_var_name(Ids::DaStart.into(), vm, ids_data, ap_tracking)?;
    let output_ptr = get_ptr_from_var_name(Ids::OutputPtr.into(), vm, ids_data, ap_tracking)?;
    let onchain_data_size = (output_ptr - onchain_data_start)?;
    let output_builtin = vm.get_output_builtin_mut()?;

    let n_pages = onchain_data_size.div_ceil(MAX_PAGE_SIZE);
    for i in 0..n_pages {
        let start_offset = i * MAX_PAGE_SIZE;
        let page_id = i + 1;
        let page_start = (onchain_data_start + start_offset)?;
        let page_size = min(onchain_data_size - start_offset, MAX_PAGE_SIZE);
        output_builtin.add_page(page_id, page_start, page_size)?;
    }
    output_builtin.add_attribute(
        OUTPUT_ATTRIBUTE_FACT_TOPOLOGY.to_string(),
        vec![
            // Push 1 + n_pages pages (all of the pages).
            1 + n_pages,
            // Create a parent node for the last n_pages.
            n_pages,
            // Don't push additional pages.
            0,
            // Take the first page (the main part) and the node that was created (onchain data)
            // and use them to construct the root of the fact tree.
            2,
        ],
    );

    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn set_state_updates_start(
    HintArgsNoHP { vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgsNoHP<'_>,
) -> OsHintResult {
    let use_kzg_da_felt =
        get_integer_from_var_name(Ids::UseKzgDa.into(), vm, ids_data, ap_tracking)?;

    // Set `use_kzg_da` in globals since it will be used in `process_data_availability`
    exec_scopes.insert_value(Scope::UseKzgDa.into(), use_kzg_da_felt);

    let compress_state_updates =
        get_integer_from_var_name(Ids::CompressStateUpdates.into(), vm, ids_data, ap_tracking)?;

    let use_kzg_da = match use_kzg_da_felt {
        x if x == Felt::ONE => Ok(true),
        x if x == Felt::ZERO => Ok(false),
        _ => Err(OsHintError::BooleanIdExpected { id: Ids::UseKzgDa, felt: use_kzg_da_felt }),
    }?;

    let use_compress_state_updates = match compress_state_updates {
        x if x == Felt::ONE => Ok(true),
        x if x == Felt::ZERO => Ok(false),
        _ => Err(OsHintError::BooleanIdExpected {
            id: Ids::CompressStateUpdates,
            felt: compress_state_updates,
        }),
    }?;

    if use_kzg_da || use_compress_state_updates {
        insert_value_from_var_name(
            Ids::StateUpdatesStart.into(),
            vm.add_memory_segment(),
            vm,
            ids_data,
            ap_tracking,
        )?;
    } else {
        // Assign a temporary segment, to be relocated into the output segment.
        insert_value_from_var_name(
            Ids::StateUpdatesStart.into(),
            vm.add_temporary_segment(),
            vm,
            ids_data,
            ap_tracking,
        )?;
    }

    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn set_compressed_start(
    HintArgsNoHP { vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgsNoHP<'_>,
) -> OsHintResult {
    let use_kzg_da_felt = exec_scopes.get::<Felt>(Scope::UseKzgDa.into())?;

    let use_kzg_da = match use_kzg_da_felt {
        x if x == Felt::ONE => Ok(true),
        x if x == Felt::ZERO => Ok(false),
        _ => Err(OsHintError::BooleanIdExpected { id: Ids::UseKzgDa, felt: use_kzg_da_felt }),
    }?;

    if use_kzg_da {
        insert_value_from_var_name(
            Ids::CompressedStart.into(),
            vm.add_memory_segment(),
            vm,
            ids_data,
            ap_tracking,
        )?;
    } else {
        // Assign a temporary segment, to be relocated into the output segment.
        insert_value_from_var_name(
            Ids::CompressedStart.into(),
            vm.add_temporary_segment(),
            vm,
            ids_data,
            ap_tracking,
        )?;
    }

    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn set_n_updates_small(
    HintArgsNoHP { vm, ids_data, ap_tracking, constants, .. }: HintArgsNoHP<'_>,
) -> OsHintResult {
    let n_updates = get_integer_from_var_name(Ids::NUpdates.into(), vm, ids_data, ap_tracking)?;
    let n_updates_small_packing_bounds =
        Const::fetch(&Const::NUpdatesSmallPackingBound, constants)?;
    insert_value_from_var_name(
        Ids::IsNUpdatesSmall.into(),
        Felt::from(&n_updates < n_updates_small_packing_bounds),
        vm,
        ids_data,
        ap_tracking,
    )?;
    Ok(())
}
