use std::collections::HashMap;

use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_maybe_relocatable_from_var_name,
    get_ptr_from_var_name,
    insert_value_from_var_name,
};
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_types_core::felt::Felt;

use super::utils::compress;
use crate::hints::error::OsHintResult;
use crate::hints::hint_implementation::stateless_compression::utils::TOTAL_N_BUCKETS;
use crate::hints::types::HintArgs;
use crate::hints::vars::{Ids, Scope};

pub(crate) fn dictionary_from_bucket(HintArgs { exec_scopes, .. }: HintArgs<'_>) -> OsHintResult {
    let initial_dict: HashMap<MaybeRelocatable, MaybeRelocatable> = (0..TOTAL_N_BUCKETS)
        .map(|bucket_index| (Felt::from(bucket_index).into(), Felt::ZERO.into()))
        .collect();
    exec_scopes.insert_box(Scope::InitialDict.into(), Box::new(initial_dict));
    Ok(())
}

pub(crate) fn get_prev_offset(
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let dict_manager = exec_scopes.get_dict_manager()?;

    let dict_ptr = get_ptr_from_var_name(Ids::DictPtr.into(), vm, ids_data, ap_tracking)?;
    let bucket_index =
        get_maybe_relocatable_from_var_name(Ids::BucketIndex.into(), vm, ids_data, ap_tracking)?;
    let prev_offset =
        dict_manager.borrow_mut().get_tracker_mut(dict_ptr)?.get_value(&bucket_index)?.clone();
    insert_value_from_var_name(Ids::PrevOffset.into(), prev_offset, vm, ids_data, ap_tracking)?;
    Ok(())
}

pub(crate) fn compression_hint(
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let data_start = get_ptr_from_var_name(Ids::DataStart.into(), vm, ids_data, ap_tracking)?;
    let data_end = get_ptr_from_var_name(Ids::DataEnd.into(), vm, ids_data, ap_tracking)?;
    let data_size = (data_end - data_start)?;

    let compressed_dst =
        get_ptr_from_var_name(Ids::CompressedDst.into(), vm, ids_data, ap_tracking)?;
    let data =
        vm.get_integer_range(data_start, data_size)?.into_iter().map(|f| *f).collect::<Vec<_>>();
    let compress_result =
        compress(&data).into_iter().map(MaybeRelocatable::Int).collect::<Vec<_>>();

    vm.write_arg(compressed_dst, &compress_result)?;

    Ok(())
}

pub(crate) fn set_decompressed_dst(
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_>,
) -> OsHintResult {
    let decompressed_dst =
        get_ptr_from_var_name(Ids::DecompressedDst.into(), vm, ids_data, ap_tracking)?;

    let packed_felt = get_integer_from_var_name(Ids::PackedFelt.into(), vm, ids_data, ap_tracking)?;
    let elm_bound = get_integer_from_var_name(Ids::ElmBound.into(), vm, ids_data, ap_tracking)?;

    vm.insert_value(
        decompressed_dst,
        packed_felt.div_rem(&elm_bound.try_into().expect("elm_bound is zero")).1,
    )?;

    Ok(())
}
