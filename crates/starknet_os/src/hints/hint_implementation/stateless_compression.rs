use std::collections::HashMap;

use blockifier::state::state_api::StateReader;
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_types_core::felt::Felt;

use crate::hints::error::HintResult;
use crate::hints::types::HintArgs;
use crate::hints::vars::Scope;

/// Number of bits encoding each element (per bucket).
const N_BITS_PER_BUCKET: [usize; 6] = [252, 125, 83, 62, 31, 15];
/// Number of buckets, including the repeating values bucket.
const TOTAL_N_BUCKETS: usize = N_BITS_PER_BUCKET.len() + 1;

pub(crate) fn dictionary_from_bucket<S: StateReader>(
    HintArgs { exec_scopes, .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    let initial_dict: HashMap<MaybeRelocatable, MaybeRelocatable> = (0..TOTAL_N_BUCKETS)
        .map(|bucket_index| (Felt::from(bucket_index).into(), Felt::ZERO.into()))
        .collect();
    exec_scopes.insert_box(Scope::InitialDict.into(), Box::new(initial_dict));
    Ok(())
}

pub(crate) fn get_prev_offset<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn compression_hint<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}

pub(crate) fn set_decompressed_dst<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_, S>,
) -> HintResult {
    todo!()
}
