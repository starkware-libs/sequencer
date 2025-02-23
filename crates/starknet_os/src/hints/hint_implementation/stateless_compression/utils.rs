use std::any::type_name;
use std::cmp::max;

use strum::EnumCount;
use strum_macros::EnumCount;

pub(crate) const N_UNIQUE_BUCKETS: usize = BitLength::COUNT;
/// Number of buckets, including the repeating values bucket.
pub(crate) const TOTAL_N_BUCKETS: usize = N_UNIQUE_BUCKETS + 1;

pub(crate) const MAX_N_BITS: usize = 251;

#[derive(Debug, thiserror::Error)]
enum ConversionError {
    #[error("{n_bits} bits for {type_name}.")]
    Overflow { n_bits: usize, type_name: String },
}

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

    fn min_bit_length(n_bits: usize) -> Result<Self, ConversionError> {
        match n_bits {
            _ if n_bits <= 15 => Ok(Self::Bits15),
            _ if n_bits <= 31 => Ok(Self::Bits31),
            _ if n_bits <= 62 => Ok(Self::Bits62),
            _ if n_bits <= 83 => Ok(Self::Bits83),
            _ if n_bits <= 125 => Ok(Self::Bits125),
            _ if n_bits <= 252 => Ok(Self::Bits252),
            _ => Err(ConversionError::Overflow {
                n_bits,
                type_name: type_name::<Self>().to_string(),
            }),
        }
    }
}
