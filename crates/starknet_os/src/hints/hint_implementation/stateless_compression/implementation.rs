use std::collections::HashMap;

use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::get_maybe_relocatable_from_var_name;
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_types_core::felt::Felt;

use super::utils::compress;
use crate::hints::error::OsHintResult;
use crate::hints::hint_implementation::stateless_compression::utils::TOTAL_N_BUCKETS;
use crate::hints::types::HintArgs;
use crate::hints::vars::{Ids, Scope};

pub(crate) fn dictionary_from_bucket(ctx: HintArgs<'_>) -> OsHintResult {
    let initial_dict: HashMap<MaybeRelocatable, MaybeRelocatable> = (0..TOTAL_N_BUCKETS)
        .map(|bucket_index| (Felt::from(bucket_index).into(), Felt::ZERO.into()))
        .collect();
    ctx.exec_scopes.insert_box(Scope::InitialDict.into(), Box::new(initial_dict));
    Ok(())
}

pub(crate) fn get_prev_offset(mut ctx: HintArgs<'_>) -> OsHintResult {
    let dict_manager = ctx.exec_scopes.get_dict_manager()?;

    let dict_ptr = ctx.get_ptr(Ids::DictPtr.into())?;
    let bucket_index = get_maybe_relocatable_from_var_name(
        Ids::BucketIndex.into(),
        ctx.vm,
        ctx.ids_data,
        ctx.ap_tracking,
    )?;
    let prev_offset =
        dict_manager.borrow_mut().get_tracker_mut(dict_ptr)?.get_value(&bucket_index)?.clone();
    ctx.insert_value(Ids::PrevOffset.into(), prev_offset)?;
    Ok(())
}

pub(crate) fn compression_hint(ctx: HintArgs<'_>) -> OsHintResult {
    let data_start = ctx.get_ptr(Ids::DataStart.into())?;
    let data_end = ctx.get_ptr(Ids::DataEnd.into())?;
    let data_size = (data_end - data_start)?;

    let compressed_dst = ctx.get_ptr(Ids::CompressedDst.into())?;
    let data = ctx
        .vm
        .get_integer_range(data_start, data_size)?
        .into_iter()
        .map(|f| *f)
        .collect::<Vec<_>>();
    let compress_result =
        compress(&data).into_iter().map(MaybeRelocatable::Int).collect::<Vec<_>>();

    ctx.vm.write_arg(compressed_dst, &compress_result)?;

    Ok(())
}

pub(crate) fn set_decompressed_dst(ctx: HintArgs<'_>) -> OsHintResult {
    let decompressed_dst = ctx.get_ptr(Ids::DecompressedDst.into())?;

    let packed_felt = ctx.get_integer(Ids::PackedFelt.into())?;
    let elm_bound = ctx.get_integer(Ids::ElmBound.into())?;

    ctx.vm.insert_value(
        decompressed_dst,
        packed_felt.div_rem(&elm_bound.try_into().expect("elm_bound is zero")).1,
    )?;

    Ok(())
}
