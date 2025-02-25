use std::collections::HashMap;

use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_maybe_relocatable_from_var_name,
    get_ptr_from_var_name,
    insert_value_from_var_name,
};
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_types_core::felt::Felt;

use crate::hints::error::HintResult;
use crate::hints::hint_implementation::stateless_compression::utils::TOTAL_N_BUCKETS;
use crate::hints::types::HintArgs;
use crate::hints::vars::{Ids, Scope};

pub(crate) fn dictionary_from_bucket<S: StateReader>(
    HintArgs { exec_scopes, .. }: HintArgs<'_, S>,
) -> HintResult {
    let initial_dict: HashMap<MaybeRelocatable, MaybeRelocatable> = (0..TOTAL_N_BUCKETS)
        .map(|bucket_index| (Felt::from(bucket_index).into(), Felt::ZERO.into()))
        .collect();
    exec_scopes.insert_box(Scope::InitialDict.into(), Box::new(initial_dict));
    Ok(())
}

pub(crate) fn get_prev_offset<S: StateReader>(
    HintArgs { vm, exec_scopes, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
) -> HintResult {
    let dict_manager = exec_scopes.get_dict_manager()?;

    let dict_ptr = get_ptr_from_var_name(Ids::DictPtr.into(), vm, ids_data, ap_tracking)?;
    let dict_tracker = dict_manager.borrow().get_tracker(dict_ptr)?.get_dictionary_copy();
    exec_scopes.insert_box(Scope::DictTracker.into(), Box::new(dict_tracker));

    let bucket_index =
        get_maybe_relocatable_from_var_name(Ids::BucketIndex.into(), vm, ids_data, ap_tracking)?;
    let prev_offset =
        dict_manager.borrow_mut().get_tracker_mut(dict_ptr)?.get_value(&bucket_index)?.clone();
    insert_value_from_var_name(Ids::PrevOffset.into(), prev_offset, vm, ids_data, ap_tracking)?;
    Ok(())
}

pub(crate) fn compression_hint<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}

pub(crate) fn set_decompressed_dst<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> HintResult {
    todo!()
}
