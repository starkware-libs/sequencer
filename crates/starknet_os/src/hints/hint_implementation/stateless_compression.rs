use std::collections::HashMap;

use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_types_core::felt::Felt;

use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::Scope;

/// Number of bits encoding each element (per bucket).
const N_BITS_PER_BUCKET: [usize; 6] = [252, 125, 83, 62, 31, 15];
/// Number of buckets, including the repeating values bucket.
const TOTAL_N_BUCKETS: usize = N_BITS_PER_BUCKET.len() + 1;

pub(crate) fn dictionary_from_bucket(
    HintArgs { exec_scopes, .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    let initial_dict: HashMap<MaybeRelocatable, MaybeRelocatable> = (0..TOTAL_N_BUCKETS)
        .map(|bucket_index| (Felt::from(bucket_index).into(), Felt::ZERO.into()))
        .collect();
    exec_scopes.insert_box(Scope::InitialDict.into(), Box::new(initial_dict));
    Ok(())
}

pub(crate) fn get_prev_offset(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>) -> HintResult {
    let dict_ptr = get_ptr_from_var_name(vars::ids::DICT_PTR, vm, ids_data, ap_tracking)?;

    let dict_tracker = match exec_scopes.get_dict_manager()?.borrow().get_tracker(dict_ptr)?.data.clone() {
        Dictionary::SimpleDictionary(hash_map) => hash_map,
        Dictionary::DefaultDictionary { dict, .. } => dict,
    };

    let bucket_index = get_maybe_relocatable_from_var_name(vars::ids::BUCKET_INDEX, vm, ids_data, ap_tracking)?;

    let prev_offset = match dict_tracker.get(&bucket_index) {
        Some(offset) => offset.clone(),
        None => return Err(custom_hint_error("No prev_offset found for the given bucket_index")),
    };

    exec_scopes.insert_box(vars::scopes::DICT_TRACKER, Box::new(dict_tracker));
    insert_value_from_var_name(vars::ids::PREV_OFFSET, prev_offset, vm, ids_data, ap_tracking)?;
    Ok(())
}

pub(crate) fn compression_hint(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub(crate) fn set_decompressed_dst(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}
