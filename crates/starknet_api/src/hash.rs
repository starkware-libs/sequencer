#[cfg(test)]
#[path = "hash_test.rs"]
mod hash_test;

use std::fmt::{Debug, Display, Formatter};

use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use starknet_types_core::felt::{Felt, FromStrError};
use starknet_types_core::hash::{Poseidon, StarkHash as StarkHashTrait};

use crate::core::{ContractAddress, EntryPointSelector, GlobalRoot, Nonce, GLOBAL_STATE_VERSION};
use crate::serde_utils::bytes_from_hex_str;
use crate::transaction::fields::Calldata;
use crate::transaction::L1HandlerTransaction;

pub type StarkHash = Felt;

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    Eq,
    PartialEq,
    Hash,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
    derive_more::Display,
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
#[derive(Clone, Copy, Debug, Deserialize, Default, PartialEq, Eq, Hash, Serialize)]
pub struct StateRoots {
    pub contracts_trie_root_hash: HashOutput,
    pub classes_trie_root_hash: HashOutput,
}

impl StateRoots {
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

/// The hash of a L1 -> L2 message, as it's stored on the Starknet Solidity contract.
// The hash is Keccak256, so it doesn't fit in a Felt.
#[derive(Clone, Default, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct EthL1L2MessageHash(pub [u8; 32]);

impl Display for EthL1L2MessageHash {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "0x{}", hex::encode(self.0))
    }
}

impl Debug for EthL1L2MessageHash {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, formatter)
    }
}

impl Serialize for EthL1L2MessageHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(format!("{self}").as_str())
    }
}

impl<'de> Deserialize<'de> for EthL1L2MessageHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self(bytes_from_hex_str::<32, true>(s.as_str()).map_err(serde::de::Error::custom)?))
    }
}

impl L1HandlerTransaction {
    pub fn calc_eth_msg_hash(&self) -> EthL1L2MessageHash {
        eth_l1_handler_message_hash(
            &self.contract_address,
            self.nonce,
            &self.entry_point_selector,
            &self.calldata,
        )
    }
}

/// Calculating the message hash of L1 -> L2 message.
pub fn eth_l1_handler_message_hash(
    contract_address: &ContractAddress,
    nonce: Nonce,
    entry_point_selector: &EntryPointSelector,
    calldata: &Calldata,
) -> EthL1L2MessageHash {
    let (from_address, payload) =
        calldata.0.split_first().expect("Invalid calldata, expected at least from_address");

    let mut encoded = Vec::new();
    encoded.extend(from_address.to_bytes_be());
    encoded.extend(contract_address.0.key().to_bytes_be());
    encoded.extend(nonce.to_bytes_be());
    encoded.extend(entry_point_selector.0.to_bytes_be());

    let payload_length_as_felt =
        Felt::from(u64::try_from(payload.len()).expect("usize should fit in u64"));
    encoded.extend(payload_length_as_felt.to_bytes_be());

    for felt in payload {
        encoded.extend(felt.to_bytes_be());
    }

    let mut keccak = Keccak256::default();
    keccak.update(&encoded);
    EthL1L2MessageHash(keccak.finalize().into())
}
