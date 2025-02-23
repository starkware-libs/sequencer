use starknet_types_core::felt::Felt;

/// Number of bits encoding each element (per bucket).
pub(crate) const N_BITS_PER_BUCKET: [usize; 6] = [252, 125, 83, 62, 31, 15];
/// Number of buckets, including the repeating values bucket.
pub(crate) const TOTAL_N_BUCKETS: usize = N_BITS_PER_BUCKET.len() + 1;

/// A struct representing a vector of bits with a specified size.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct SizedBitsVec(pub(crate) Vec<bool>);

impl SizedBitsVec {
    pub fn from_felt(felt: Felt, n_bits: usize) -> Self {
        Self(felt.to_bits_le()[0..n_bits].to_vec())
    }
}

impl From<SizedBitsVec> for Felt {
    fn from(val: SizedBitsVec) -> Self {
        bits_to_felt(val.0.as_ref())
    }
}

fn bits_to_felt(bits: &[bool]) -> Felt {
    let mut bytes = [0_u8; 32];
    for (byte_idx, chunk) in bits.chunks(8).enumerate() {
        let mut byte = 0_u8;
        for (bit_idx, bit) in chunk.iter().enumerate() {
            if *bit {
                byte |= 1 << bit_idx;
            }
        }
        bytes[byte_idx] = byte;
    }
    Felt::from_bytes_le(&bytes)
}
