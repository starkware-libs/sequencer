#[cfg(test)]
#[path = "core_test.rs"]
mod core_test;

use std::fmt::Debug;

use derive_more::Display;
use once_cell::sync::Lazy;
use primitive_types::H160;
use serde::{Deserialize, Serialize};
use starknet_crypto::FieldElement;

use crate::crypto::PublicKey;
use crate::hash::{pedersen_hash_array, PoseidonHash, StarkFelt, StarkHash};
use crate::serde_utils::{BytesAsHex, PrefixedBytesAsHex};
use crate::transaction::{Calldata, ContractAddressSalt};
use crate::{impl_from_through_intermediate, StarknetApiError};

/// A chain id.
#[derive(Clone, Debug, Display, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct ChainId(pub String);

impl ChainId {
    pub fn as_hex(&self) -> String {
        format!("0x{}", hex::encode(&self.0))
    }
}

/// The address of a contract, used for example in [StateDiff](`crate::state::StateDiff`),
/// [DeclareTransaction](`crate::transaction::DeclareTransaction`), and
/// [BlockHeader](`crate::block::BlockHeader`).

// The block hash table is stored in address 0x1,
// this is a special address that is not used for contracts.
pub const BLOCK_HASH_TABLE_ADDRESS: ContractAddress = ContractAddress(PatriciaKey(StarkHash::ONE));
#[derive(
    Debug,
    Default,
    Copy,
    Clone,
    Eq,
    PartialEq,
    Hash,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
    derive_more::Deref,
)]
pub struct ContractAddress(pub PatriciaKey);

impl From<ContractAddress> for StarkFelt {
    fn from(contract_address: ContractAddress) -> StarkFelt {
        **contract_address
    }
}

impl From<u128> for ContractAddress {
    fn from(val: u128) -> Self {
        ContractAddress(PatriciaKey::from(val))
    }
}

impl_from_through_intermediate!(u128, ContractAddress, u8, u16, u32, u64);

/// The maximal size of storage var.
pub const MAX_STORAGE_ITEM_SIZE: u16 = 256;
/// The prefix used in the calculation of a contract address.
pub const CONTRACT_ADDRESS_PREFIX: &str = "STARKNET_CONTRACT_ADDRESS";
/// The size of the contract address domain.
pub static CONTRACT_ADDRESS_DOMAIN_SIZE: Lazy<StarkFelt> = Lazy::new(|| {
    StarkFelt::try_from(PATRICIA_KEY_UPPER_BOUND)
        .unwrap_or_else(|_| panic!("Failed to convert {PATRICIA_KEY_UPPER_BOUND} to StarkFelt"))
});
/// The address upper bound; it is defined to be congruent with the storage var address upper bound.
pub static L2_ADDRESS_UPPER_BOUND: Lazy<FieldElement> = Lazy::new(|| {
    FieldElement::from(*CONTRACT_ADDRESS_DOMAIN_SIZE) - FieldElement::from(MAX_STORAGE_ITEM_SIZE)
});

impl TryFrom<StarkHash> for ContractAddress {
    type Error = StarknetApiError;
    fn try_from(hash: StarkHash) -> Result<Self, Self::Error> {
        Ok(Self(PatriciaKey::try_from(hash)?))
    }
}

// TODO: Add a hash_function as a parameter
pub fn calculate_contract_address(
    salt: ContractAddressSalt,
    class_hash: ClassHash,
    constructor_calldata: &Calldata,
    deployer_address: ContractAddress,
) -> Result<ContractAddress, StarknetApiError> {
    let constructor_calldata_hash = pedersen_hash_array(&constructor_calldata.0);
    let contract_address_prefix = format!("0x{}", hex::encode(CONTRACT_ADDRESS_PREFIX));
    let mut address = FieldElement::from(pedersen_hash_array(&[
        StarkFelt::try_from(contract_address_prefix.as_str())?,
        *deployer_address.0.key(),
        salt.0,
        class_hash.0,
        constructor_calldata_hash,
    ]));
    address = address % *L2_ADDRESS_UPPER_BOUND;

    ContractAddress::try_from(StarkFelt::from(address))
}

/// The hash of a ContractClass.
#[derive(
    Debug,
    Default,
    Copy,
    Clone,
    Eq,
    PartialEq,
    Hash,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
    Display,
    derive_more::Deref,
)]
pub struct ClassHash(pub StarkHash);

/// The hash of a compiled ContractClass.
#[derive(
    Debug,
    Default,
    Copy,
    Clone,
    Eq,
    PartialEq,
    Hash,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
    Display,
)]
pub struct CompiledClassHash(pub StarkHash);

/// A general type for nonces.
#[derive(
    Debug,
    Default,
    Copy,
    Clone,
    Eq,
    PartialEq,
    Hash,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
    derive_more::Deref,
)]
pub struct Nonce(pub StarkFelt);

impl Nonce {
    pub fn try_increment(&self) -> Result<Self, StarknetApiError> {
        let current_nonce = FieldElement::from(self.0);

        // Check if an overflow occurred during increment.
        match StarkFelt::from(current_nonce + FieldElement::ONE) {
            StarkFelt::ZERO => Err(StarknetApiError::OutOfRange { string: format!("{:?}", self) }),
            incremented_felt => Ok(Self(incremented_felt)),
        }
    }
}

/// The selector of an [EntryPoint](`crate::deprecated_contract_class::EntryPoint`).
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct EntryPointSelector(pub StarkHash);

/// The root of the global state at a [Block](`crate::block::Block`)
/// and [StateUpdate](`crate::state::StateUpdate`).
#[derive(
    Debug,
    Copy,
    Clone,
    Default,
    Eq,
    PartialEq,
    Hash,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
    Display,
)]
pub struct GlobalRoot(pub StarkHash);

/// The commitment on the transactions in a [Block](`crate::block::Block`).
#[derive(
    Debug,
    Copy,
    Clone,
    Default,
    Eq,
    PartialEq,
    Hash,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
    Display,
)]
pub struct TransactionCommitment(pub StarkHash);

/// The commitment on the events in a [Block](`crate::block::Block`).
#[derive(
    Debug,
    Copy,
    Clone,
    Default,
    Eq,
    PartialEq,
    Hash,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
    Display,
)]
pub struct EventCommitment(pub StarkHash);

/// The commitment on the receipts in a [Block](`crate::block::Block`).
#[derive(
    Debug,
    Copy,
    Clone,
    Default,
    Eq,
    PartialEq,
    Hash,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
    Display,
)]
pub struct ReceiptCommitment(pub StarkHash);

#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct StateDiffCommitment(pub PoseidonHash);

/// A key for nodes of a Patricia tree.
// Invariant: key is in range.
#[derive(
    Copy,
    Clone,
    Eq,
    PartialEq,
    Default,
    Hash,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
    derive_more:: Deref,
)]
pub struct PatriciaKey(StarkHash);

// 2**251
pub const PATRICIA_KEY_UPPER_BOUND: &str =
    "0x800000000000000000000000000000000000000000000000000000000000000";

impl PatriciaKey {
    pub fn key(&self) -> &StarkHash {
        &self.0
    }
}

impl From<u128> for PatriciaKey {
    fn from(val: u128) -> Self {
        PatriciaKey::try_from(StarkFelt::from(val)).expect("Failed to convert u128 to PatriciaKey.")
    }
}

impl_from_through_intermediate!(u128, PatriciaKey, u8, u16, u32, u64);

impl TryFrom<StarkHash> for PatriciaKey {
    type Error = StarknetApiError;

    fn try_from(value: StarkHash) -> Result<Self, Self::Error> {
        if value < *CONTRACT_ADDRESS_DOMAIN_SIZE {
            return Ok(PatriciaKey(value));
        }
        Err(StarknetApiError::OutOfRange { string: format!("[0x0, {PATRICIA_KEY_UPPER_BOUND})") })
    }
}

impl Debug for PatriciaKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PatriciaKey").field(&self.0).finish()
    }
}

/// A utility macro to create a [`PatriciaKey`] from a hex string / unsigned integer representation.
#[cfg(any(feature = "testing", test))]
#[macro_export]
macro_rules! patricia_key {
    ($s:expr) => {
        PatriciaKey::try_from(StarkHash::try_from($s).unwrap()).unwrap()
    };
}

/// A utility macro to create a [`ClassHash`] from a hex string / unsigned integer representation.
#[cfg(any(feature = "testing", test))]
#[macro_export]
macro_rules! class_hash {
    ($s:expr) => {
        ClassHash(StarkHash::try_from($s).unwrap())
    };
}

/// A utility macro to create a [`ContractAddress`] from a hex string / unsigned integer
/// representation.
#[cfg(any(feature = "testing", test))]
#[macro_export]
macro_rules! contract_address {
    ($s:expr) => {
        ContractAddress(patricia_key!($s))
    };
}

/// An Ethereum address.
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(try_from = "PrefixedBytesAsHex<20_usize>", into = "PrefixedBytesAsHex<20_usize>")]
pub struct EthAddress(pub H160);

impl TryFrom<StarkFelt> for EthAddress {
    type Error = StarknetApiError;
    fn try_from(felt: StarkFelt) -> Result<Self, Self::Error> {
        const COMPLIMENT_OF_H160: usize = std::mem::size_of::<StarkFelt>() - H160::len_bytes();

        let (rest, h160_bytes) = felt.bytes().split_at(COMPLIMENT_OF_H160);
        if rest != [0u8; COMPLIMENT_OF_H160] {
            return Err(StarknetApiError::OutOfRange { string: felt.to_string() });
        }

        Ok(EthAddress(H160::from_slice(h160_bytes)))
    }
}

impl From<EthAddress> for StarkFelt {
    fn from(value: EthAddress) -> Self {
        let mut bytes = [0u8; 32];
        // Padding H160 with zeros to 32 bytes (big endian)
        bytes[12..32].copy_from_slice(value.0.as_bytes());
        StarkFelt::new_unchecked(bytes)
    }
}

impl TryFrom<PrefixedBytesAsHex<20_usize>> for EthAddress {
    type Error = StarknetApiError;
    fn try_from(val: PrefixedBytesAsHex<20_usize>) -> Result<Self, Self::Error> {
        Ok(EthAddress(H160::from_slice(&val.0)))
    }
}

impl From<EthAddress> for PrefixedBytesAsHex<20_usize> {
    fn from(felt: EthAddress) -> Self {
        BytesAsHex(felt.0.to_fixed_bytes())
    }
}

/// A public key of a sequencer.
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct SequencerPublicKey(pub PublicKey);

#[derive(
    Debug, Default, Clone, Copy, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct SequencerContractAddress(pub ContractAddress);
