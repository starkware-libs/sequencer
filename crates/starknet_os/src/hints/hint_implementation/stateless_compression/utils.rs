use std::cmp::max;

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
