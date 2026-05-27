use cairo_vm::hint_processor::builtin_hint_processor::blake2s_hash::IV as SHA256_IV;
use cairo_vm::types::relocatable::MaybeRelocatable;
use sha2::digest::generic_array::{ArrayLength, GenericArray};
use starknet_types_core::felt::Felt;

#[cfg(test)]
#[path = "utils_test.rs"]
pub mod utils_test;

pub(crate) const N_MISSING_BLOCKS_BOUND: u32 = 20;
pub(crate) const SHA256_INPUT_CHUNK_SIZE_BOUND: usize = 100;
pub(crate) const SHA512_INPUT_CHUNK_SIZE_BOUND: usize = 100;

// SHA-512 initial hash values (FIPS 180-4, §5.3.5). sha2::consts is private in sha2 0.10.x.
pub(crate) const SHA512_IV: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];

trait ToBeBytes<const N: usize> {
    fn to_be_bytes(self) -> [u8; N];
}

impl ToBeBytes<4> for u32 {
    fn to_be_bytes(self) -> [u8; 4] {
        self.to_be_bytes()
    }
}

impl ToBeBytes<8> for u64 {
    fn to_be_bytes(self) -> [u8; 8] {
        self.to_be_bytes()
    }
}

fn calculate_sha_padding<U, L, const N: usize>(
    iv: [U; 8],
    compress_function: fn(&mut [U; 8], &[GenericArray<u8, L>]),
    sha_input_chunk_size_felts: usize,
    number_of_missing_blocks: u32,
) -> Vec<MaybeRelocatable>
where
    U: Clone + Copy + From<u8> + ToBeBytes<N>,
    Felt: From<U>,
    L: ArrayLength<u8>,
{
    let message: Vec<U> = vec![U::from(0_u8); sha_input_chunk_size_felts];
    let flat_message = GenericArray::from_exact_iter(message.iter().flat_map(|v| v.to_be_bytes()))
        .unwrap_or_else(|| panic!("Failed to create a dummy message."));
    let mut initial_state = iv;
    compress_function(&mut initial_state, &[flat_message]);
    let padding_to_repeat: Vec<U> =
        [message, iv.to_vec(), initial_state.to_vec()].into_iter().flatten().collect();

    let mut padding = vec![];
    let padding_extension =
        padding_to_repeat.iter().map(|x| MaybeRelocatable::from(Felt::from(*x)));
    for _ in 0..number_of_missing_blocks {
        padding.extend(padding_extension.clone());
    }
    padding
}

pub(crate) fn calculate_sha256_padding(
    sha_input_chunk_size_felts: usize,
    number_of_missing_blocks: u32,
) -> Vec<MaybeRelocatable> {
    calculate_sha_padding(
        SHA256_IV,
        sha2::compress256,
        sha_input_chunk_size_felts,
        number_of_missing_blocks,
    )
}

pub(crate) fn calculate_sha512_padding(
    sha_input_chunk_size_felts: usize,
    number_of_missing_blocks: u32,
) -> Vec<MaybeRelocatable> {
    calculate_sha_padding(
        SHA512_IV,
        sha2::compress512,
        sha_input_chunk_size_felts,
        number_of_missing_blocks,
    )
}
