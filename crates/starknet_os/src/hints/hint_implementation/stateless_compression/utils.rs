use std::{cmp::max, hash::Hash};

use num_bigint::BigUint;
use num_traits::{ToPrimitive, Zero};
use starknet_types_core::felt::Felt;
use strum::EnumCount;
use strum_macros::EnumCount;

pub(crate) const COMPRESSION_VERSION: u8 = 0;
pub(crate) const HEADER_ELM_N_BITS: usize = 20;
pub(crate) const HEADER_ELM_BOUND: usize = 1 << HEADER_ELM_N_BITS;

pub(crate) const N_UNIQUE_BUCKETS: usize = Buckets::n_buckets();
/// Number of buckets, including the repeating values bucket.
pub(crate) const TOTAL_N_BUCKETS: usize = N_UNIQUE_BUCKETS + 1;

pub(crate) const MAX_N_BITS: usize = 251;

#[derive(Debug, EnumCount)]
pub(crate) enum BitLength {
    Bits15,
    Bits31,
    Bits62,
    Bits83,
    Bits125,
    Bits252,
}

impl BitLength {
    pub(crate) const fn n_bits(&self) -> usize {
        match self {
            BitLength::Bits15 => 15,
            BitLength::Bits31 => 31,
            BitLength::Bits62 => 62,
            BitLength::Bits83 => 83,
            BitLength::Bits125 => 125,
            BitLength::Bits252 => 252,
        }
    }

    pub(crate) fn n_elems_in_felt(&self) -> usize {
        max(MAX_N_BITS / self.n_bits(), 1)
    }

    pub(crate) fn from_n_bits(n_bits: usize) -> Self {
        if n_bits <= 15 {
            BitLength::Bits15
        } else if n_bits <= 31 {
            BitLength::Bits31
        } else if n_bits <= 62 {
            BitLength::Bits62
        } else if n_bits <= 83 {
            BitLength::Bits83
        } else if n_bits <= 125 {
            BitLength::Bits125
        } else {
            BitLength::Bits252
        }
    }
}

pub(crate) trait BucketElementTrait: Sized {
    fn to_bit_length() -> BitLength;

    fn as_bool_ref(&self) -> &[bool];

    fn pack_in_felts(elms: &[Self]) -> Vec<Felt> {
        elms.chunks(Self::to_bit_length().n_elems_in_felt())
            .map(|chunk| {
                bits_to_felt(
                    &(chunk.iter().flat_map(Self::as_bool_ref).copied().collect::<Vec<_>>()),
                )
            })
            .collect()
    }
}

/// A struct representing a vector of bits with a specified size.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct BitsArray<const LENGTH: usize>(pub(crate) [bool; LENGTH]);

impl<const LENGTH: usize> TryFrom<Felt> for BitsArray<LENGTH> {
    type Error = String;

    fn try_from(felt: Felt) -> Result<Self, Self::Error> {
        match felt.to_bits_le()[0..LENGTH].try_into() {
            Ok(bits_vec) => Ok(Self(bits_vec)),
            Err(_) => {
                Err(format!("Too many bits in Felt to convert to BitsArray<{:?}>", LENGTH))
            }
        }
    }
}

impl<const LENGTH: usize> From<BitsArray<LENGTH>> for Felt {
    fn from(bits_array: BitsArray<LENGTH>) -> Self {
        bits_to_felt(&bits_array.0)
    }
}

pub(crate) type BucketElement15 = BitsArray<15>;
pub(crate) type BucketElement31 = BitsArray<31>;
pub(crate) type BucketElement62 = BitsArray<62>;
pub(crate) type BucketElement83 = BitsArray<83>;
pub(crate) type BucketElement125 = BitsArray<125>;
pub(crate) type BucketElement252 = BitsArray<252>;


macro_rules! impl_bucket_element_trait {
    ($bucket_element:ident, $bit_length:ident) => {
        impl BucketElementTrait for $bucket_element {
            fn to_bit_length() -> BitLength {
                BitLength::$bit_length
            }

            fn as_bool_ref(&self) -> &[bool] {
                &self.0.as_ref()
            }
        }
    };
}

impl_bucket_element_trait!(BucketElement15, Bits15);
impl_bucket_element_trait!(BucketElement31, Bits31);
impl_bucket_element_trait!(BucketElement62, Bits62);
impl_bucket_element_trait!(BucketElement83, Bits83);
impl_bucket_element_trait!(BucketElement125, Bits125);
impl_bucket_element_trait!(BucketElement252, Bits252);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum BucketElement {
    BucketElement15(BucketElement15),
    BucketElement31(BucketElement31),
    BucketElement62(BucketElement62),
    BucketElement83(BucketElement83),
    BucketElement125(BucketElement125),
    BucketElement252(BucketElement252),
}

impl BucketElement {
    pub(crate) fn as_bool_ref(&self) -> &[bool] {
        match self {
            BucketElement::BucketElement15(sized_bits_vec) => sized_bits_vec.as_bool_ref(),
            BucketElement::BucketElement31(sized_bits_vec) => sized_bits_vec.as_bool_ref(),
            BucketElement::BucketElement62(sized_bits_vec) => sized_bits_vec.as_bool_ref(),
            BucketElement::BucketElement83(sized_bits_vec) => sized_bits_vec.as_bool_ref(),
            BucketElement::BucketElement125(sized_bits_vec) => sized_bits_vec.as_bool_ref(),
            BucketElement::BucketElement252(sized_bits_vec) => sized_bits_vec.as_bool_ref(),
        }
    }
}

impl From<Felt> for BucketElement {
    fn from(felt: Felt) -> Self {
        match BitLength::from_n_bits(felt.bits()) {
            BitLength::Bits15 => BucketElement::BucketElement15(felt.try_into().unwrap()),
            BitLength::Bits31 => BucketElement::BucketElement31(felt.try_into().unwrap()),
            BitLength::Bits62 => BucketElement::BucketElement62(felt.try_into().unwrap()),
            BitLength::Bits83 => BucketElement::BucketElement83(felt.try_into().unwrap()),
            BitLength::Bits125 => BucketElement::BucketElement125(felt.try_into().unwrap()),
            BitLength::Bits252 => BucketElement::BucketElement252(felt.try_into().unwrap()),
        }
    }
}

impl From<BucketElement> for Felt {
    fn from(val: BucketElement) -> Self {
        bits_to_felt(val.as_bool_ref())
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

/// A set-like data structure that preserves the insertion order.
/// Holds values of `n_bits` for bit length representation.
#[derive(Default, Clone, Debug)]
struct UniqueValueBucket<SizedElement: BucketElementTrait> {
    value_to_index: indexmap::IndexMap<SizedElement, usize>,
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

    fn pack_in_felts(&self) -> Vec<Felt> {
        let values = self.value_to_index.keys().cloned().collect::<Vec<_>>();
        SizedElement::pack_in_felts(&values)
    }
}

#[derive(Clone, Debug)]
struct Buckets {
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

    const fn n_buckets() -> usize {
        BitLength::COUNT
    }

    fn bucket_index(&self, bucket_element: &BucketElement) -> (usize, usize) {
        let bucket_index = match bucket_element {
            BucketElement::BucketElement15(_) => 0,
            BucketElement::BucketElement31(_) => 1,
            BucketElement::BucketElement62(_) => 2,
            BucketElement::BucketElement83(_) => 3,
            BucketElement::BucketElement125(_) => 4,
            BucketElement::BucketElement252(_) => 5,
        };
        (bucket_index, 5 - bucket_index)
    }

    fn get_element_index(&self, bucket_element: &BucketElement) -> Option<&usize> {
        match bucket_element {
            BucketElement::BucketElement15(bucket_element15) => self.bucket15.get_index(&bucket_element15),
            BucketElement::BucketElement31(bucket_element31) => self.bucket31.get_index(&bucket_element31),
            BucketElement::BucketElement62(bucket_element62) => self.bucket62.get_index(&bucket_element62),
            BucketElement::BucketElement83(bucket_element83) => self.bucket83.get_index(&bucket_element83),
            BucketElement::BucketElement125(bucket_element125) => self.bucket125.get_index(&bucket_element125),
            BucketElement::BucketElement252(bucket_element252) => self.bucket252.get_index(&bucket_element252),
        }
    }

    fn add(&mut self, bucket_element: BucketElement) {
        match bucket_element {
            BucketElement::BucketElement15(bucket_element15) => self.bucket15.add(bucket_element15),
            BucketElement::BucketElement31(bucket_element31) => self.bucket31.add(bucket_element31),
            BucketElement::BucketElement62(bucket_element62) => self.bucket62.add(bucket_element62),
            BucketElement::BucketElement83(bucket_element83) => self.bucket83.add(bucket_element83),
            BucketElement::BucketElement125(bucket_element125) => self.bucket125.add(bucket_element125),
            BucketElement::BucketElement252(bucket_element252) => self.bucket252.add(bucket_element252),
        }
    }

    fn lengths(&self) -> Vec<usize> {
        [
            self.bucket252.len(),
            self.bucket125.len(),
            self.bucket83.len(),
            self.bucket62.len(),
            self.bucket31.len(),
            self.bucket15.len(),
        ].into()
    }

    fn pack_in_felts(&self) -> Vec<Felt> {
        [
            self.bucket252.pack_in_felts(),
            self.bucket125.pack_in_felts(),
            self.bucket83.pack_in_felts(),
            self.bucket62.pack_in_felts(),
            self.bucket31.pack_in_felts(),
            self.bucket15.pack_in_felts(),
        ].into_iter().flatten().collect()
    }
}

/// A utility class for compression.
/// Used to manage and store the unique values in separate buckets according to their bit length.
#[derive(Clone, Debug)]
pub(crate) struct CompressionSet {
    buckets: Buckets,
    repeating_value_locations: Vec<(usize, usize)>,
    bucket_index_per_elm: Vec<usize>,
    finalized: bool,
}

impl CompressionSet {
    /// Creates a new Compression set given an array of the n_bits per each bucket in the set.
    pub fn new() -> Self {
        Self {
            buckets: Buckets::new(),
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
        Buckets::n_buckets()
    }

    /// Iterates over the provided values and assigns each value to the appropriate bucket based on
    /// the number of bits required to represent it. If a value is already in a bucket, it is
    /// recorded as a repeating value. Otherwise, it is added to the appropriate bucket.
    pub fn update(&mut self, values: &[Felt]) {
        assert!(!self.finalized, "Cannot add values after finalizing.");

        for value in values {
            let bucket_element = BucketElement::from(*value);
            let (bucket_index, reversed_bucket_index) = self.buckets.bucket_index(&bucket_element);
            if let Some(element_index) = self.buckets.get_element_index(&bucket_element) {
                self.repeating_value_locations.push((bucket_index, *element_index));
                self.bucket_index_per_elm.push(self.repeating_values_bucket_index());
            } else {
                self.buckets.add(bucket_element.clone());
                self.bucket_index_per_elm.push(reversed_bucket_index);
            }
        }
    }

    pub fn get_unique_value_bucket_lengths(&self) -> Vec<usize> {
        self.buckets.lengths()
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
        self.buckets.pack_in_felts()
    }

    pub fn finalize(&mut self) {
        self.finalized = true;
    }
}

/// Compresses the data provided to output a Vec of compressed Felts.
pub(crate) fn compress(data: &[Felt]) -> Vec<Felt> {
    assert!(data.len() < HEADER_ELM_BOUND.to_usize().unwrap(), "Data is too long.");

    let mut compression_set = CompressionSet::new();
    compression_set.update(data);
    compression_set.finalize();

    let bucket_index_per_elm = compression_set.get_bucket_index_per_elm();
    println!("bucket_index_per_elm: {:?}", bucket_index_per_elm);
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
    println!("packed_bucket_index_per_elm: {:?}", packed_bucket_index_per_elm);
    result.extend(packed_header);
    result.extend(unique_values);
    result.extend(packed_repeating_value_pointers);
    result.extend(packed_bucket_index_per_elm);
    result
}

fn bits_required(elm_bound: usize) -> usize {
    if elm_bound == 0 {
        1 // Zero requires 1 bit to represent
    } else {
        usize::try_from(usize::BITS - elm_bound.leading_zeros())
            .expect("usize overflow")
    }
}

/// Calculates the number of elements that can fit in a single felt value, given the element bound.
pub fn get_n_elms_per_felt(elm_bound: usize) -> usize {
    let n_bits_required = bits_required(elm_bound);
    if n_bits_required > MAX_N_BITS {
        return 1;
    }
    MAX_N_BITS / n_bits_required
}

/// Packs a list of elements into multiple felts, ensuring that each felt contains as many elements
/// as can fit.
pub fn pack_usize_in_felts(elms: &[usize], elm_bound: usize) -> Vec<Felt> {
    elms.chunks(get_n_elms_per_felt(elm_bound))
        .map(|chunk| pack_usize_in_felt(chunk, elm_bound))
        .collect()
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
