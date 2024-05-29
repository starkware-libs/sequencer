use std::fmt::Debug;

use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use starknet_types_core::felt::Felt;

pub type StarkHash = Felt;

#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct PoseidonHash(pub Felt);

/// Computes the first 250 bits of the Keccak256 hash, in order to fit into a field element.
pub fn starknet_keccak_hash(input: &[u8]) -> Felt {
    let mut keccak = Keccak256::default();
    keccak.update(input);
    let mut hashed_bytes: [u8; 32] = keccak.finalize().into();
    hashed_bytes[0] &= 0b00000011_u8; // Discard the six MSBs.
    Felt::from_bytes_be(&hashed_bytes)
}

#[cfg(any(feature = "testing", test))]
pub struct FeltConverter;

#[cfg(any(feature = "testing", test))]
pub trait TryIntoFelt<V> {
    fn to_felt_unchecked(v: V) -> Felt;
}

#[cfg(any(feature = "testing", test))]
impl TryIntoFelt<u128> for FeltConverter {
    fn to_felt_unchecked(v: u128) -> Felt {
        Felt::from(v)
    }
}

#[cfg(any(feature = "testing", test))]
impl TryIntoFelt<&str> for FeltConverter {
    fn to_felt_unchecked(v: &str) -> Felt {
        Felt::from_hex_unchecked(v)
    }
}

/// A utility macro to create a [`starknet_types_core::felt::Felt`] from an intergert or a hex
/// string representation.
#[cfg(any(feature = "testing", test))]
#[macro_export]
macro_rules! felt {
    ($s:expr) => {
        FeltConverter::to_felt_unchecked($s)
    };
}
