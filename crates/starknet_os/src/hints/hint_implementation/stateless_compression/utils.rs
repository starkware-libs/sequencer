use std::any::type_name;
use std::cmp::max;

use starknet_types_core::felt::Felt;
use strum::EnumCount;
use strum_macros::EnumCount;

use crate::hints::error::OsHintError;

pub(crate) const N_UNIQUE_BUCKETS: usize = BitLength::COUNT;
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
    const MAX: usize = 252;

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

type BucketElement15 = BitsArray<15>;
type BucketElement31 = BitsArray<31>;
type BucketElement62 = BitsArray<62>;
type BucketElement83 = BitsArray<83>;
pub(crate) type BucketElement125 = BitsArray<125>;
type BucketElement252 = BitsArray<252>;

/// Panics in case the length is 252 bits and the value is larger than max Felt.
fn felt_from_bits_le(bits: &[bool]) -> Result<Felt, OsHintError> {
    if bits.len() > BitLength::MAX {
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
    fn to_bit_length() -> BitLength;

    fn as_bool_ref(&self) -> &[bool];

    fn pack_in_felts(elms: &[Self]) -> Result<Vec<Felt>, OsHintError> {
        elms.chunks(Self::to_bit_length().n_elems_in_felt())
            .map(|chunk| {
                felt_from_bits_le(
                    &(chunk.iter().flat_map(Self::as_bool_ref).copied().collect::<Vec<_>>()),
                )
            })
            .collect()
    }
}

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
            BitLength::Bits252 => {
                BucketElement::BucketElement252(felt.try_into().expect("Up to 252 bits"))
            }
        }
    }
}
