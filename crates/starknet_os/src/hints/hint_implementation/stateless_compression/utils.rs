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

/// A set-like data structure that preserves the insertion order.
/// Holds values of `n_bits` for bit length representation.
#[derive(Default, Clone, Debug)]
struct UniqueValueBucket {
    n_bits: usize,
    value_to_index: indexmap::IndexMap<SizedBitsVec, usize>,
}

impl UniqueValueBucket {
    /// `n_bits` is an individual value associated with a specific bucket,
    /// that specifies the maximum number of bits that values in that bucket can have.
    fn new(n_bits: usize) -> Self {
        Self { n_bits, value_to_index: Default::default() }
    }

    fn contains(&self, value: &SizedBitsVec) -> bool {
        self.value_to_index.contains_key(value)
    }

    fn len(&self) -> usize {
        self.value_to_index.len()
    }

    fn add(&mut self, value: SizedBitsVec) {
        if !self.contains(&value) {
            let next_index = self.value_to_index.len();
            self.value_to_index.insert(value, next_index);
        }
    }

    fn get_index(&self, value: &SizedBitsVec) -> usize {
        *self.value_to_index.get(value).expect("The value provided is not in the index")
    }

    fn pack_in_felts(&self) -> Vec<Felt> {
        let values = self.value_to_index.keys().cloned().collect::<Vec<_>>();
        pack_in_felts(&values, self.n_bits)
    }
}

/// Computes the starting offsets for each bucket in a list of buckets, based on their lengths.
pub(crate) fn get_bucket_offsets(bucket_lengths: &[usize]) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(bucket_lengths.len());
    let mut current = 0;

    for &length in bucket_lengths {
        offsets.push(current);
        current += length;
    }

    offsets
}
