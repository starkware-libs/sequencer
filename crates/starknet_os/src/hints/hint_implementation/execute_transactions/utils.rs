use cairo_vm::hint_processor::builtin_hint_processor::blake2s_hash::IV;
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_types_core::felt::Felt;

#[cfg(test)]
#[path = "utils_test.rs"]
pub mod utils_test;
pub(crate) const N_MISSING_BLOCKS_BOUND: u32 = 20;
pub(crate) const SHA256_INPUT_CHUNK_SIZE_BOUND: usize = 100;

pub(crate) fn calculate_padding(
    sha256_input_chunk_size_felts: usize,
    number_of_missing_blocks: u32,
) -> Vec<MaybeRelocatable> {
    let message = vec![0_u32; sha256_input_chunk_size_felts];
    let flat_message = sha2::digest::generic_array::GenericArray::from_exact_iter(
        message.iter().flat_map(|v| v.to_be_bytes()),
    )
    .expect("Failed to create a dummy message for sha2_finalize.");
    let mut initial_state = IV;
    sha2::compress256(&mut initial_state, &[flat_message]);
    let padding_to_repeat: Vec<u32> =
        [message, IV.to_vec(), initial_state.to_vec()].into_iter().flatten().collect();

    let mut padding = vec![];
    let padding_extension =
        padding_to_repeat.iter().map(|x| MaybeRelocatable::from(Felt::from(*x)));
    for _ in 0..number_of_missing_blocks {
        padding.extend(padding_extension.clone());
    }
    padding
}
