use std::fmt::Debug;

use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use starknet_types_core::felt::{Felt, FromStrError};
use starknet_types_core::hash::{Poseidon, StarkHash as StarkHashTrait};

use crate::core::{GlobalRoot, GLOBAL_STATE_VERSION};

pub type StarkHash = Felt;

#[derive(
    Debug, Clone, Copy, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
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

macro_rules! impl_try_into_felt {
    ($type:ty) => {
        #[cfg(any(feature = "testing", test))]
        impl TryIntoFelt<$type> for FeltConverter {
            fn to_felt_unchecked(v: $type) -> Felt {
                Felt::from(v)
            }
        }
    };
}

impl_try_into_felt!(u128);
impl_try_into_felt!(u64);
impl_try_into_felt!(u32);
impl_try_into_felt!(u16);
impl_try_into_felt!(u8);

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
        <$crate::hash::FeltConverter as $crate::hash::TryIntoFelt<_>>::to_felt_unchecked($s)
    };
}

/// A [Felt] wrapper representing the output of a hash function.
#[derive(Clone, Copy, Debug, Deserialize, Default, PartialEq, Eq, Hash, Serialize)]
pub struct HashOutput(pub Felt);

impl HashOutput {
    pub const ROOT_OF_EMPTY_TREE: Self = Self(Felt::ZERO);
    pub fn from_hex(hex_string: &str) -> Result<Self, FromStrError> {
        Ok(HashOutput(Felt::from_hex(hex_string)?))
    }
}

/// Output of committing a state.
pub struct CommitmentOutput {
    pub contracts_trie_root_hash: HashOutput,
    pub classes_trie_root_hash: HashOutput,
}

impl CommitmentOutput {
    pub fn global_root(&self) -> GlobalRoot {
        if self.contracts_trie_root_hash == HashOutput::ROOT_OF_EMPTY_TREE
            && self.classes_trie_root_hash == HashOutput::ROOT_OF_EMPTY_TREE
        {
            return GlobalRoot::ROOT_OF_EMPTY_STATE;
        }
        GlobalRoot(Poseidon::hash_array(&[
            GLOBAL_STATE_VERSION,
            self.contracts_trie_root_hash.0,
            self.classes_trie_root_hash.0,
        ]))
    }
}
