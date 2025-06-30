use std::any::type_name;
use std::cmp::{max, min};
use std::hash::Hash;

use indexmap::IndexMap;
use num_bigint::BigUint;
use num_integer::Integer;
use num_traits::{ToPrimitive, Zero};
use starknet_types_core::felt::Felt;
use strum::EnumCount;
use strum_macros::Display;

use crate::hints::error::OsHintError;

pub(crate) const COMPRESSION_VERSION: u8 = 0;
pub(crate) const HEADER_ELM_N_BITS: usize = 20;
pub(crate) const HEADER_ELM_BOUND: u32 = 1 << HEADER_ELM_N_BITS;

pub(crate) const N_UNIQUE_BUCKETS: usize = BitLength::COUNT;
/// Number of buckets, including the repeating values bucket.
pub(crate) const TOTAL_N_BUCKETS: usize = N_UNIQUE_BUCKETS + 1;

pub(crate) const MAX_N_BITS: usize = 251;
const HEADER_LEN: usize = 1 + 1 + TOTAL_N_BUCKETS;

#[derive(Debug, Display, strum_macros::EnumCount)]
pub(crate) enum BitLength {
    Bits15,
    Bits31,
    Bits62,
    Bits83,
    Bits125,
    Bits252,
}

impl BitLength {
    const fn n_bits(&self) -> usize {
        match self {
            Self::Bits15 => 15,
            Self::Bits31 => 31,
            Self::Bits62 => 62,
            Self::Bits83 => 83,
            Self::Bits125 => 125,
            Self::Bits252 => 252,
        }
    }

    pub(crate) fn n_elems_in_felt(&self) -> usize {
        max(MAX_N_BITS / self.n_bits(), 1)
    }

    pub(crate) fn min_bit_length(n_bits: usize) -> Result<Self, OsHintError> {
        match n_bits {
            _ if n_bits <= 15 => Ok(Self::Bits15),
            _ if n_bits <= 31 => Ok(Self::Bits31),
            _ if n_bits <= 62 => Ok(Self::Bits62),
            _ if n_bits <= 83 => Ok(Self::Bits83),
            _ if n_bits <= 125 => Ok(Self::Bits125),
            _ if n_bits <= 252 => Ok(Self::Bits252),
            _ => Err(OsHintError::StatelessCompressionOverflow {
                n_bits,
                type_name: type_name::<Self>().to_string(),
            }),
        }
    }
}

/// A struct representing a vector of bits with a specified size.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct BitsArray<const LENGTH: usize>(pub(crate) [bool; LENGTH]);

impl<const LENGTH: usize> TryFrom<Felt> for BitsArray<LENGTH> {
    type Error = OsHintError;

    // Cloned the first `LENGTH` bits of the felt.
    fn try_from(felt: Felt) -> Result<Self, Self::Error> {
        let n_bits_felt = felt.bits();
        if n_bits_felt > LENGTH {
            return Err(Self::Error::StatelessCompressionOverflow {
                n_bits: n_bits_felt,
                type_name: type_name::<Self>().to_string(),
            });
        }
        Ok(Self(felt.to_bits_le()[0..LENGTH].try_into().expect("Too many bits in Felt")))
    }
}

impl<const LENGTH: usize> TryFrom<BitsArray<LENGTH>> for Felt {
    type Error = OsHintError;

    fn try_from(bits_array: BitsArray<LENGTH>) -> Result<Self, Self::Error> {
        felt_from_bits_le(&bits_array.0)
    }
}

pub(crate) type BucketElement15 = BitsArray<15>;
pub(crate) type BucketElement31 = BitsArray<31>;
pub(crate) type BucketElement62 = BitsArray<62>;
pub(crate) type BucketElement83 = BitsArray<83>;
pub(crate) type BucketElement125 = BitsArray<125>;
pub(crate) type BucketElement252 = Felt;

/// Returns an error in case the length is not guaranteed to fit in Felt (more than 251 bits).
pub(crate) fn felt_from_bits_le(bits: &[bool]) -> Result<Felt, OsHintError> {
    if bits.len() > MAX_N_BITS {
        return Err(OsHintError::StatelessCompressionOverflow {
            n_bits: bits.len(),
            type_name: type_name::<Felt>().to_string(),
        });
    }

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
    Ok(Felt::from_bytes_le(&bytes))
}

pub(crate) trait BucketElementTrait: Sized {
    fn pack_in_felts(elms: &[Self]) -> Vec<Felt>;
}

macro_rules! impl_bucket_element_trait {
    ($bucket_element:ident, $bit_length:ident) => {
        impl BucketElementTrait for $bucket_element {
            fn pack_in_felts(elms: &[Self]) -> Vec<Felt> {
                let bit_length = BitLength::$bit_length;
                elms.chunks(bit_length.n_elems_in_felt())
                    .map(|chunk| {
                        felt_from_bits_le(
                            &(chunk
                                .iter()
                                .flat_map(|elem| elem.0.as_ref())
                                .copied()
                                .collect::<Vec<_>>()),
                        )
                        .unwrap_or_else(|_| {
                            panic!(
                                "Chunks of size {}, each of bit length {}, fit in felts.",
                                bit_length.n_elems_in_felt(),
                                bit_length
                            )
                        })
                    })
                    .collect()
            }
        }
    };
}

impl_bucket_element_trait!(BucketElement15, Bits15);
impl_bucket_element_trait!(BucketElement31, Bits31);
impl_bucket_element_trait!(BucketElement62, Bits62);
impl_bucket_element_trait!(BucketElement83, Bits83);
impl_bucket_element_trait!(BucketElement125, Bits125);

impl BucketElementTrait for BucketElement252 {
    fn pack_in_felts(elms: &[Self]) -> Vec<Felt> {
        elms.to_vec()
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum BucketElement {
    BucketElement15(BucketElement15),
    BucketElement31(BucketElement31),
    BucketElement62(BucketElement62),
    BucketElement83(BucketElement83),
    BucketElement125(BucketElement125),
    BucketElement252(BucketElement252),
}

impl From<Felt> for BucketElement {
    fn from(felt: Felt) -> Self {
        match BitLength::min_bit_length(felt.bits()).expect("felt is up to 252 bits") {
            BitLength::Bits15 => {
                BucketElement::BucketElement15(felt.try_into().expect("Up to 15 bits"))
            }
            BitLength::Bits31 => {
                BucketElement::BucketElement31(felt.try_into().expect("Up to 31 bits"))
            }
            BitLength::Bits62 => {
                BucketElement::BucketElement62(felt.try_into().expect("Up to 62 bits"))
            }
            BitLength::Bits83 => {
                BucketElement::BucketElement83(felt.try_into().expect("Up to 83 bits"))
            }
            BitLength::Bits125 => {
                BucketElement::BucketElement125(felt.try_into().expect("Up to 125 bits"))
            }
            BitLength::Bits252 => BucketElement::BucketElement252(felt),
        }
    }
}

/// Holds IndexMap of unique values of the same size in bits.
#[derive(Default, Clone, Debug)]
struct UniqueValueBucket<SizedElement: BucketElementTrait> {
    value_to_index: IndexMap<SizedElement, usize>,
}

impl<SizedElement: BucketElementTrait + Clone + Eq + Hash> UniqueValueBucket<SizedElement> {
    fn new() -> Self {
        Self { value_to_index: Default::default() }
    }

    fn len(&self) -> usize {
        self.value_to_index.len()
    }

    fn contains(&self, value: &SizedElement) -> bool {
        self.value_to_index.contains_key(value)
    }

    fn add(&mut self, value: SizedElement) {
        if !self.contains(&value) {
            let next_index = self.value_to_index.len();
            self.value_to_index.insert(value, next_index);
        }
    }

    fn get_index(&self, value: &SizedElement) -> Option<&usize> {
        self.value_to_index.get(value)
    }

    fn pack_in_felts(self) -> Vec<Felt> {
        let values = self.value_to_index.into_keys().collect::<Vec<_>>();
        SizedElement::pack_in_felts(&values)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Buckets {
    bucket15: UniqueValueBucket<BucketElement15>,
    bucket31: UniqueValueBucket<BucketElement31>,
    bucket62: UniqueValueBucket<BucketElement62>,
    bucket83: UniqueValueBucket<BucketElement83>,
    bucket125: UniqueValueBucket<BucketElement125>,
    bucket252: UniqueValueBucket<BucketElement252>,
}

impl Buckets {
    pub(crate) fn new() -> Self {
        Self {
            bucket15: UniqueValueBucket::new(),
            bucket31: UniqueValueBucket::new(),
            bucket62: UniqueValueBucket::new(),
            bucket83: UniqueValueBucket::new(),
            bucket125: UniqueValueBucket::new(),
            bucket252: UniqueValueBucket::new(),
        }
    }

    fn get_inverse_bucket_index(&self, bucket_element: &BucketElement) -> usize {
        let bucket_index = match bucket_element {
            BucketElement::BucketElement15(_) => 0,
            BucketElement::BucketElement31(_) => 1,
            BucketElement::BucketElement62(_) => 2,
            BucketElement::BucketElement83(_) => 3,
            BucketElement::BucketElement125(_) => 4,
            BucketElement::BucketElement252(_) => 5,
        };
        N_UNIQUE_BUCKETS - 1 - bucket_index
    }

    /// Returns the index of the element in the respective bucket.
    pub(crate) fn get_element_index(&self, bucket_element: &BucketElement) -> Option<&usize> {
        match bucket_element {
            BucketElement::BucketElement15(bucket_element15) => {
                self.bucket15.get_index(bucket_element15)
            }
            BucketElement::BucketElement31(bucket_element31) => {
                self.bucket31.get_index(bucket_element31)
            }
            BucketElement::BucketElement62(bucket_element62) => {
                self.bucket62.get_index(bucket_element62)
            }
            BucketElement::BucketElement83(bucket_element83) => {
                self.bucket83.get_index(bucket_element83)
            }
            BucketElement::BucketElement125(bucket_element125) => {
                self.bucket125.get_index(bucket_element125)
            }
            BucketElement::BucketElement252(bucket_element252) => {
                self.bucket252.get_index(bucket_element252)
            }
        }
    }

    pub(crate) fn add(&mut self, bucket_element: BucketElement) {
        match bucket_element {
            BucketElement::BucketElement15(bucket_element15) => self.bucket15.add(bucket_element15),
            BucketElement::BucketElement31(bucket_element31) => self.bucket31.add(bucket_element31),
            BucketElement::BucketElement62(bucket_element62) => self.bucket62.add(bucket_element62),
            BucketElement::BucketElement83(bucket_element83) => self.bucket83.add(bucket_element83),
            BucketElement::BucketElement125(bucket_element125) => {
                self.bucket125.add(bucket_element125)
            }
            BucketElement::BucketElement252(bucket_element252) => {
                self.bucket252.add(bucket_element252)
            }
        }
    }

    /// Returns the lengths of the buckets from the bucket with the largest numbers to the bucket
    /// with the smallest.
    pub(crate) fn lengths(&self) -> [usize; N_UNIQUE_BUCKETS] {
        [
            self.bucket252.len(),
            self.bucket125.len(),
            self.bucket83.len(),
            self.bucket62.len(),
            self.bucket31.len(),
            self.bucket15.len(),
        ]
    }

    /// Chains the buckets from the bucket with the largest numbers to the bucket with the smallest,
    /// and packed them into felts.
    fn pack_in_felts(self) -> Vec<Felt> {
        [
            self.bucket15.pack_in_felts(),
            self.bucket31.pack_in_felts(),
            self.bucket62.pack_in_felts(),
            self.bucket83.pack_in_felts(),
            self.bucket125.pack_in_felts(),
            self.bucket252.pack_in_felts(),
        ]
        .into_iter()
        .rev()
        .flatten()
        .collect()
    }
}

/// A utility class for compression.
/// Used to manage and store the unique values in separate buckets according to their bit length.
#[derive(Clone, Debug)]
pub(crate) struct CompressionSet {
    unique_value_buckets: Buckets,
    /// For each repeating value, holds the unique bucket index and the index of the element in the
    /// bucket.
    repeating_value_bucket: Vec<(usize, usize)>,
    /// For each element(including the repeating values), holds the containing bucket index.
    bucket_index_per_elm: Vec<usize>,
}

impl CompressionSet {
    /// Creates a new Compression set.
    /// Iterates over the provided values and assigns each value to the appropriate bucket based on
    /// the number of bits required to represent it. If a value is already in a bucket, it is
    /// recorded as a repeating value. Otherwise, it is added to the appropriate bucket.
    pub fn new(values: &[Felt]) -> Self {
        let mut obj = Self {
            unique_value_buckets: Buckets::new(),
            repeating_value_bucket: Vec::new(),
            bucket_index_per_elm: Vec::new(),
        };
        let repeating_values_bucket_index = N_UNIQUE_BUCKETS;

        for value in values {
            let bucket_element = BucketElement::from(*value);
            let inverse_bucket_index =
                obj.unique_value_buckets.get_inverse_bucket_index(&bucket_element);
            if let Some(element_index) = obj.unique_value_buckets.get_element_index(&bucket_element)
            {
                obj.repeating_value_bucket.push((inverse_bucket_index, *element_index));
                obj.bucket_index_per_elm.push(repeating_values_bucket_index);
            } else {
                obj.unique_value_buckets.add(bucket_element.clone());
                obj.bucket_index_per_elm.push(inverse_bucket_index);
            }
        }
        obj
    }

    pub fn get_unique_value_bucket_lengths(&self) -> [usize; N_UNIQUE_BUCKETS] {
        self.unique_value_buckets.lengths()
    }

    pub fn n_repeating_values(&self) -> usize {
        self.repeating_value_bucket.len()
    }

    /// Converts the repeating value locations from (bucket, index) to a global index in the chained
    /// buckets.
    pub fn get_repeating_value_pointers(&self) -> Vec<usize> {
        let unique_value_bucket_lengths = self.get_unique_value_bucket_lengths();
        let bucket_offsets = get_bucket_offsets(&unique_value_bucket_lengths);

        self.repeating_value_bucket
            .iter()
            .map(|&(bucket_index, index_in_bucket)| bucket_offsets[bucket_index] + index_in_bucket)
            .collect()
    }

    pub fn pack_unique_values(self) -> Vec<Felt> {
        self.unique_value_buckets.pack_in_felts()
    }
}

/// Compresses the data provided to output a Vec of compressed Felts.
pub(crate) fn compress(data: &[Felt]) -> Vec<Felt> {
    assert!(
        data.len() < HEADER_ELM_BOUND.to_usize().expect("usize overflow"),
        "Data is too long: {} >= {HEADER_ELM_BOUND}.",
        data.len()
    );

    let compression_set = CompressionSet::new(data);

    let unique_value_bucket_lengths = compression_set.get_unique_value_bucket_lengths();
    let n_unique_values: usize = unique_value_bucket_lengths.iter().sum();

    let header: Vec<usize> = [COMPRESSION_VERSION.into(), data.len()]
        .into_iter()
        .chain(unique_value_bucket_lengths)
        .chain([compression_set.n_repeating_values()])
        .collect();

    let packed_header = pack_usize_in_felts(&header, HEADER_ELM_BOUND);
    let packed_repeating_value_pointers = pack_usize_in_felts(
        &compression_set.get_repeating_value_pointers(),
        u32::try_from(n_unique_values).expect("Too many unique values"),
    );
    let packed_bucket_index_per_elm = pack_usize_in_felts(
        &compression_set.bucket_index_per_elm,
        u32::try_from(TOTAL_N_BUCKETS).expect("Too many buckets"),
    );

    let unique_values = compression_set.pack_unique_values();
    [packed_header, unique_values, packed_repeating_value_pointers, packed_bucket_index_per_elm]
        .into_iter()
        .flatten()
        .collect()
}

/// Calculates the number of elements with the same bit length as the element bound, that can fit
/// into a single felt value.
pub fn get_n_elms_per_felt(elm_bound: u32) -> usize {
    if elm_bound <= 1 {
        return MAX_N_BITS;
    }
    let n_bits_required = elm_bound.ilog2() + 1;
    MAX_N_BITS / usize::try_from(n_bits_required).expect("usize overflow")
}

/// Packs a list of elements into multiple felts, ensuring that each felt contains as many elements
/// as can fit.
pub fn pack_usize_in_felts(elms: &[usize], elm_bound: u32) -> Vec<Felt> {
    elms.chunks(get_n_elms_per_felt(elm_bound))
        .map(|chunk| pack_usize_in_felt(chunk, elm_bound))
        .collect()
}

/// Packs a chunk of elements into a single felt. Assumes that the elms fit in a felt.
fn pack_usize_in_felt(elms: &[usize], elm_bound: u32) -> Felt {
    let elm_bound_as_big = BigUint::from(elm_bound);
    elms.iter()
        .enumerate()
        .fold(BigUint::zero(), |acc, (i, elm)| {
            acc + BigUint::from(*elm) * elm_bound_as_big.pow(u32::try_from(i).expect("fit u32"))
        })
        .into()
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

pub fn unpack_felts<const LENGTH: usize>(
    compressed: &[Felt],
    n_elms: usize,
) -> Vec<BitsArray<LENGTH>> {
    let n_elms_per_felt = BitLength::min_bit_length(LENGTH).unwrap().n_elems_in_felt();
    let mut result = Vec::with_capacity(n_elms);

    for felt in compressed {
        let n_packed_elms = min(n_elms_per_felt, n_elms - result.len());
        for chunk in felt.to_bits_le()[0..n_packed_elms * LENGTH].chunks_exact(LENGTH) {
            result.push(BitsArray(chunk.try_into().unwrap()));
        }
    }

    result
}

pub fn unpack_felts_to_usize(compressed: &[Felt], n_elms: usize, elm_bound: u32) -> Vec<usize> {
    let n_elms_per_felt = get_n_elms_per_felt(elm_bound);
    let elm_bound_as_big = BigUint::from(elm_bound);
    let mut result = Vec::with_capacity(n_elms);

    for felt in compressed {
        let mut remaining = felt.to_biguint();
        let n_packed_elms = min(n_elms_per_felt, n_elms - result.len());
        for _ in 0..n_packed_elms {
            let (new_remaining, value) = remaining.div_rem(&elm_bound_as_big);
            result.push(value.to_usize().unwrap());
            remaining = new_remaining;
        }
    }

    result
}

/// Decompresses the given compressed data.
#[allow(dead_code)]
pub fn decompress(compressed: &mut impl Iterator<Item = Felt>) -> Vec<Felt> {
    fn unpack_chunk<const LENGTH: usize>(
        compressed: &mut impl Iterator<Item = Felt>,
        n_elms: usize,
    ) -> Vec<Felt> {
        let n_elms_per_felt = BitLength::min_bit_length(LENGTH).unwrap().n_elems_in_felt();
        let n_packed_felts = n_elms.div_ceil(n_elms_per_felt);
        let compressed_chunk: Vec<_> = compressed.take(n_packed_felts).collect();
        unpack_felts(&compressed_chunk, n_elms)
            .into_iter()
            .map(|bits: BitsArray<LENGTH>| felt_from_bits_le(&bits.0).unwrap())
            .collect()
    }

    fn unpack_chunk_to_usize(
        compressed: &mut impl Iterator<Item = Felt>,
        n_elms: usize,
        elm_bound: u32,
    ) -> Vec<usize> {
        let n_elms_per_felt = get_n_elms_per_felt(elm_bound);
        let n_packed_felts = n_elms.div_ceil(n_elms_per_felt);

        let compressed_chunk: Vec<_> = compressed.take(n_packed_felts).collect();
        unpack_felts_to_usize(&compressed_chunk, n_elms, elm_bound)
    }

    let header = unpack_chunk_to_usize(compressed, HEADER_LEN, HEADER_ELM_BOUND);
    let version = &header[0];
    assert!(version == &usize::from(COMPRESSION_VERSION), "Unsupported compression version.");

    let data_len = &header[1];
    let unique_value_bucket_lengths: Vec<usize> = header[2..2 + N_UNIQUE_BUCKETS].to_vec();
    let n_repeating_values = &header[2 + N_UNIQUE_BUCKETS];

    let mut unique_values = Vec::new();
    unique_values.extend(compressed.take(unique_value_bucket_lengths[0])); // 252 bucket.
    unique_values.extend(unpack_chunk::<125>(compressed, unique_value_bucket_lengths[1]));
    unique_values.extend(unpack_chunk::<83>(compressed, unique_value_bucket_lengths[2]));
    unique_values.extend(unpack_chunk::<62>(compressed, unique_value_bucket_lengths[3]));
    unique_values.extend(unpack_chunk::<31>(compressed, unique_value_bucket_lengths[4]));
    unique_values.extend(unpack_chunk::<15>(compressed, unique_value_bucket_lengths[5]));

    let repeating_value_pointers = unpack_chunk_to_usize(
        compressed,
        *n_repeating_values,
        unique_values.len().try_into().unwrap(),
    );

    let repeating_values: Vec<_> =
        repeating_value_pointers.iter().map(|ptr| unique_values[*ptr]).collect();

    let mut all_values = unique_values;
    all_values.extend(repeating_values);

    let bucket_index_per_elm: Vec<usize> =
        unpack_chunk_to_usize(compressed, *data_len, TOTAL_N_BUCKETS.try_into().unwrap());

    let all_bucket_lengths: Vec<usize> =
        unique_value_bucket_lengths.into_iter().chain([*n_repeating_values]).collect();

    let bucket_offsets = get_bucket_offsets(&all_bucket_lengths);

    let mut bucket_offset_trackers: Vec<_> = bucket_offsets;

    let mut result = Vec::new();
    for bucket_index in bucket_index_per_elm {
        let offset = &mut bucket_offset_trackers[bucket_index];
        let value = all_values[*offset];
        *offset += 1;
        result.push(value);
    }
    result
}
