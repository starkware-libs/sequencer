use num_bigint::BigUint;
use num_traits::{ToPrimitive, Zero};
use starknet_types_core::felt::Felt;

pub(crate) const COMPRESSION_VERSION: u8 = 0;
pub(crate) const HEADER_ELM_N_BITS: usize = 20;
pub(crate) const HEADER_ELM_BOUND: usize = 1 << HEADER_ELM_N_BITS;

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

/// A utility class for compression.
/// Used to manage and store the unique values in separate buckets according to their bit length.
#[derive(Default, Clone, Debug)]
pub(crate) struct CompressionSet {
    buckets: Vec<UniqueValueBucket>,
    sorted_buckets: Vec<(usize, UniqueValueBucket)>,
    repeating_value_locations: Vec<(usize, usize)>,
    bucket_index_per_elm: Vec<usize>,
    finalized: bool,
}

impl CompressionSet {
    /// Creates a new Compression set given an array of the n_bits per each bucket in the set.
    pub fn new(n_bits_per_bucket: &[usize]) -> Self {
        let buckets: Vec<UniqueValueBucket> =
            n_bits_per_bucket.iter().map(|&n_bits| UniqueValueBucket::new(n_bits)).collect();

        let mut sorted_buckets: Vec<(usize, UniqueValueBucket)> =
            buckets.clone().into_iter().enumerate().collect();

        sorted_buckets.sort_by_key(|(_, bucket)| bucket.n_bits);
        Self {
            buckets,
            sorted_buckets,
            repeating_value_locations: Vec::new(),
            bucket_index_per_elm: Vec::new(),
            finalized: false,
        }
    }

    /// Returns the bucket indices of the added values.
    fn get_bucket_index_per_elm(&self) -> Vec<usize> {
        assert!(self.finalized, "Cannot get bucket_index_per_elm before finalizing.");
        self.bucket_index_per_elm.clone()
    }

    fn repeating_values_bucket_index(&self) -> usize {
        self.buckets.len()
    }

    /// Iterates over the provided values and assigns each value to the appropriate bucket based on
    /// the number of bits required to represent it. If a value is already in a bucket, it is
    /// recorded as a repeating value. Otherwise, it is added to the appropriate bucket.
    pub fn update(&mut self, values: &[Felt]) {
        assert!(!self.finalized, "Cannot add values after finalizing.");

        for value in values {
            for (bucket_index, bucket) in &mut self.sorted_buckets {
                if value.bits() <= bucket.n_bits {
                    let bits_value = SizedBitsVec::from_felt(*value, bucket.n_bits);
                    if bucket.contains(&bits_value) {
                        self.repeating_value_locations
                            .push((*bucket_index, bucket.get_index(&bits_value)));
                        self.bucket_index_per_elm.push(self.repeating_values_bucket_index());
                    } else {
                        self.buckets[*bucket_index].add(bits_value.clone());
                        bucket.add(bits_value.clone());
                        self.bucket_index_per_elm.push(*bucket_index);
                    }
                    break;
                }
            }
        }
    }

    pub fn get_unique_value_bucket_lengths(&self) -> Vec<usize> {
        self.buckets.iter().map(|bucket| bucket.len()).collect()
    }

    pub fn get_repeating_value_bucket_length(&self) -> usize {
        self.repeating_value_locations.len()
    }

    /// Returns a list of BigUint corresponding to the repeating values.
    /// The BigUint point to the chained unique value buckets.
    pub fn get_repeating_value_pointers(&self) -> Vec<usize> {
        assert!(self.finalized, "Cannot get pointers before finalizing.");

        let unique_value_bucket_lengths = self.get_unique_value_bucket_lengths();
        let bucket_offsets = get_bucket_offsets(&unique_value_bucket_lengths);

        self.repeating_value_locations
            .iter()
            .map(|&(bucket_index, index_in_bucket)| bucket_offsets[bucket_index] + index_in_bucket)
            .collect()
    }

    pub fn pack_unique_values(&self) -> Vec<Felt> {
        assert!(self.finalized, "Cannot pack before finalizing.");
        self.buckets.iter().flat_map(|bucket| bucket.pack_in_felts()).collect()
    }

    pub fn finalize(&mut self) {
        self.finalized = true;
    }
}

/// Compresses the data provided to output a Vec of compressed Felts.
pub(crate) fn compress(data: &[Felt]) -> Vec<Felt> {
    assert!(data.len() < HEADER_ELM_BOUND.to_usize().unwrap(), "Data is too long.");

    let mut compression_set = CompressionSet::new(&N_BITS_PER_BUCKET);
    compression_set.update(data);
    compression_set.finalize();

    let bucket_index_per_elm = compression_set.get_bucket_index_per_elm();
    let unique_value_bucket_lengths = compression_set.get_unique_value_bucket_lengths();
    let n_unique_values: usize = unique_value_bucket_lengths.iter().sum();

    let mut header: Vec<usize> = vec![COMPRESSION_VERSION.into(), data.len()];
    header.extend(unique_value_bucket_lengths);
    header.push(compression_set.get_repeating_value_bucket_length());

    let packed_header = pack_usize_in_felts(&header, HEADER_ELM_BOUND);
    let packed_repeating_value_pointers =
        pack_usize_in_felts(&compression_set.get_repeating_value_pointers(), n_unique_values);
    let packed_bucket_index_per_elm = pack_usize_in_felts(&bucket_index_per_elm, TOTAL_N_BUCKETS);

    let unique_values = compression_set.pack_unique_values();
    let mut result = Vec::new();
    result.extend(packed_header);
    result.extend(unique_values);
    result.extend(packed_repeating_value_pointers);
    result.extend(packed_bucket_index_per_elm);
    result
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
