/// Array that specifies the number of bits allocated to each bucket.
/// Values requiring fewer bits will be placed in smaller-bit buckets,
/// and values requiring more bits will be placed in larger-bit buckets.
const N_BITS_PER_BUCKET: [usize; 6] = [252, 125, 83, 62, 31, 15];
const TOTAL_N_BUCKETS: usize = N_BITS_PER_BUCKET.len() + 1;

pub(crate) fn dictionary_from_bucket(
    HintArgs { exec_scopes, .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    let initial_dict: HashMap<MaybeRelocatable, MaybeRelocatable> =
        (0..TOTAL_N_BUCKETS).map(|bucket_index| (Felt252::from(bucket_index).into(), Felt252::ZERO.into())).collect();
    exec_scopes.insert_box(vars::scopes::INITIAL_DICT, Box::new(initial_dict));
    Ok(())
}

pub(crate) fn get_prev_offset(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub(crate) fn compression_hint(HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>) -> HintResult {
    todo!()
}

pub(crate) fn set_decompressed_dst(
    HintArgs { .. }: HintArgs<'_, '_, '_, '_, '_, '_>,
) -> HintResult {
    todo!()
}
