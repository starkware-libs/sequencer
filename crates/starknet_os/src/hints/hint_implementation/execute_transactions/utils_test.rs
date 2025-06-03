use cairo_vm::types::relocatable::MaybeRelocatable;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use super::calculate_padding;
use crate::hints::hint_implementation::execute_transactions::utils::N_MISSING_BLOCKS_BOUND;

#[rstest]
fn test_calculate_padding(
    #[values(3, 1, 0, N_MISSING_BLOCKS_BOUND - 1)] number_of_missing_blocks: u32,
) {
    // The expected single padding is independent of the number of missing blocks.
    let expected_single_padding: Vec<_> = [
        0_u32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1779033703, 3144134277, 1013904242,
        2773480762, 1359893119, 2600822924, 528734635, 1541459225, 3663108286, 398046313,
        1647531929, 2006957770, 2363872401, 3235013187, 3137272298, 406301144,
    ]
    .iter()
    .map(|x| MaybeRelocatable::from(Felt::from(*x)))
    .collect();
    let expected_padding: Vec<_> =
        (0..number_of_missing_blocks).flat_map(|_| expected_single_padding.clone()).collect();
    let sha256_input_chunk_size_felts = 16;
    let padding = calculate_padding(sha256_input_chunk_size_felts, number_of_missing_blocks);
    assert_eq!(padding, expected_padding);
}
