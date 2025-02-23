use std::cmp::max;

use starknet_types_core::felt::Felt;
use strum::EnumCount;
use strum_macros::EnumCount;

pub(crate) const N_UNIQUE_BUCKETS: usize = BitLength::n_lengths();
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
    const fn n_bits(&self) -> usize {
        match self {
            BitLength::Bits15 => 15,
            BitLength::Bits31 => 31,
            BitLength::Bits62 => 62,
            BitLength::Bits83 => 83,
            BitLength::Bits125 => 125,
            BitLength::Bits252 => 252,
        }
    }

    fn n_elems_in_felt(&self) -> usize {
        max(MAX_N_BITS / self.n_bits(), 1)
    }

    fn min_bit_length(n_bits: usize) -> Self {
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
        } else if n_bits <= 252 {
            BitLength::Bits252
        } else {
            panic!("Too many bits for BitLength {n_bits}")
        }
    }

    const fn n_lengths() -> usize {
        BitLength::COUNT
    }
}

/// A struct representing a vector of bits with a specified size.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct BitsArray<const LENGTH: usize>(pub(crate) [bool; LENGTH]);

impl<const LENGTH: usize> TryFrom<Felt> for BitsArray<LENGTH> {
    type Error = String;

    // Cloned the first `LENGTH` bits of the felt.
    fn try_from(felt: Felt) -> Result<Self, Self::Error> {
        match felt.to_bits_le()[0..LENGTH].try_into() {
            Ok(bits_vec) => Ok(Self(bits_vec)),
            Err(_) => Err(format!("Too many bits in Felt to convert to BitsArray<{:?}>", LENGTH)),
        }
    }
}

impl<const LENGTH: usize> From<BitsArray<LENGTH>> for Felt {
    /// Panics in case this value does not fit to Felt.
    fn from(bits_array: BitsArray<LENGTH>) -> Self {
        bits_to_felt(&bits_array.0)
    }
}

type BucketElement15 = BitsArray<15>;
type BucketElement31 = BitsArray<31>;
type BucketElement62 = BitsArray<62>;
type BucketElement83 = BitsArray<83>;
type BucketElement125 = BitsArray<125>;
type BucketElement252 = BitsArray<252>;

/// Panics in case this value does not fit to Felt.
fn bits_to_felt(bits: &[bool]) -> Felt {
    assert!(bits.len() <= 256, "Converts too many bits into Felt");
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
