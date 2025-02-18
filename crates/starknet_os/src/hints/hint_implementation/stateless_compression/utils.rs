use num_bigint::BigUint;
use num_traits::Zero;
use starknet_types_core::felt::Felt;

/// Number of bits encoding each element (per bucket).
pub(crate) const N_BITS_PER_BUCKET: [usize; 6] = [252, 125, 83, 62, 31, 15];
/// Number of buckets, including the repeating values bucket.
pub(crate) const TOTAL_N_BUCKETS: usize = N_BITS_PER_BUCKET.len() + 1;

pub(crate) const MAX_N_BITS: usize = 251;

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

/// Represents an upper bound for encoding elements in a compressed format.
#[derive(Debug)]
pub(crate) enum UpperBound {
    NBits(usize),
    BiggestNum(usize),
}

impl UpperBound {
    fn bits_required(&self) -> usize {
        match self {
            UpperBound::NBits(n_bits) => *n_bits,
            UpperBound::BiggestNum(biggest_num) => {
                if *biggest_num == 0 {
                    1 // Zero requires 1 bit to represent
                } else {
                    usize::try_from(usize::BITS - biggest_num.leading_zeros())
                        .expect("usize overflow")
                }
            }
        }
    }
}

/// Calculates the number of elements that can fit in a single felt value, given the element bound.
pub fn get_n_elms_per_felt(upper_bound: UpperBound) -> usize {
    let n_bits_required = upper_bound.bits_required();
    if n_bits_required > MAX_N_BITS {
        return 1;
    }
    MAX_N_BITS / n_bits_required
}

/// Packs a list of elements into multiple felts, ensuring that each felt contains as many elements
/// as can fit.
pub fn pack_in_felts(elms: &[SizedBitsVec], n_bits: usize) -> Vec<Felt> {
    elms.chunks(get_n_elms_per_felt(UpperBound::NBits(n_bits))).map(pack_in_felt).collect()
}

/// Packs a list of elements into multiple felts, ensuring that each felt contains as many elements
/// as can fit.
pub fn pack_usize_in_felts(elms: &[usize], elm_bound: usize) -> Vec<Felt> {
    elms.chunks(get_n_elms_per_felt(UpperBound::BiggestNum(elm_bound)))
        .map(|chunk| pack_usize_in_felt(chunk, elm_bound))
        .collect()
}

/// Packs a chunk of elements into a single felt.
pub fn pack_in_felt(elms: &[SizedBitsVec]) -> Felt {
    let mut combined = Vec::<bool>::new();
    elms.iter().for_each(|elem| combined.extend(elem.0.iter()));
    bits_to_felt(&combined)
}

/// Packs a chunk of elements into a single felt.
pub fn pack_usize_in_felt(elms: &[usize], elm_bound: usize) -> Felt {
    let elm_bound_as_big = BigUint::from(elm_bound);
    elms.iter()
        .enumerate()
        .fold(BigUint::zero(), |acc, (i, elm)| {
            acc + BigUint::from(*elm) * elm_bound_as_big.pow(i as u32)
        })
        .into()
}
