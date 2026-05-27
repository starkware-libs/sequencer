use cairo_vm::types::relocatable::MaybeRelocatable;
use rstest::rstest;
use starknet_types_core::felt::Felt;

use super::{calculate_sha256_padding, calculate_sha512_padding, SHA512_IV};
use crate::hints::hint_implementation::execute_transactions::utils::N_MISSING_BLOCKS_BOUND;

#[rstest]
fn test_calculate_sha256_padding(
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
    let padding = calculate_sha256_padding(sha256_input_chunk_size_felts, number_of_missing_blocks);
    assert_eq!(padding, expected_padding);
}

#[rstest]
fn test_calculate_sha512_padding(
    #[values(3, 1, 0, N_MISSING_BLOCKS_BOUND - 1)] number_of_missing_blocks: u32,
) {
    let sha512_input_chunk_size_felts = 16;
    let padding = calculate_sha512_padding(sha512_input_chunk_size_felts, number_of_missing_blocks);

    // Each block: 16 message words + 8 IV words + 8 compressed-state words = 32 felts.
    let block_size = sha512_input_chunk_size_felts + 2 * 8;
    let number_of_missing_blocks_usize = usize::try_from(number_of_missing_blocks).unwrap();
    assert_eq!(padding.len(), block_size * number_of_missing_blocks_usize);

    if number_of_missing_blocks == 0 {
        return;
    }

    let first_block = &padding[..block_size];

    // First 16 felts are zero (all-zero dummy message).
    for felt in &first_block[..sha512_input_chunk_size_felts] {
        assert_eq!(*felt, MaybeRelocatable::from(Felt::ZERO));
    }

    // Next 8 felts are the SHA-512 initial hash values.
    for (index, iv_word) in SHA512_IV.iter().enumerate() {
        assert_eq!(
            first_block[sha512_input_chunk_size_felts + index],
            MaybeRelocatable::from(Felt::from(*iv_word))
        );
    }

    // Every block is identical (the same dummy triple is repeated).
    for block_index in 1..number_of_missing_blocks_usize {
        let block = &padding[block_index * block_size..(block_index + 1) * block_size];
        assert_eq!(first_block, block);
    }
}
