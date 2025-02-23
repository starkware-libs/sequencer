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
enum BitLength {
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

    fn n_elems_in_felt(&self) -> usize {
        max(MAX_N_BITS / self.n_bits(), 1)
    }

    fn min_bit_length(n_bits: usize) -> Result<Self, OsHintError> {
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
        bits_to_felt(&bits_array.0)
    }
}

type BucketElement15 = BitsArray<15>;
type BucketElement31 = BitsArray<31>;
type BucketElement62 = BitsArray<62>;
type BucketElement83 = BitsArray<83>;
type BucketElement125 = BitsArray<125>;
type BucketElement252 = BitsArray<252>;

/// Panics in case the length is 252 bits and the value is larger than max Felt.
fn bits_to_felt(bits: &[bool]) -> Result<Felt, OsHintError> {
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
