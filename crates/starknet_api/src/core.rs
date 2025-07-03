#[cfg(test)]
#[path = "core_test.rs"]
mod core_test;

use std::fmt::Debug;
use std::sync::LazyLock;

use num_traits::ToPrimitive;
use primitive_types::H160;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sizeof::SizeOf;
use starknet_types_core::felt::{Felt, NonZeroFelt};
use starknet_types_core::hash::{Pedersen, StarkHash as CoreStarkHash};

use crate::crypto::utils::PublicKey;
use crate::hash::{PoseidonHash, StarkHash};
use crate::serde_utils::{BytesAsHex, PrefixedBytesAsHex};
use crate::transaction::fields::{Calldata, ContractAddressSalt};
use crate::{impl_from_through_intermediate, StarknetApiError};

/// Felt.
pub fn ascii_as_felt(ascii_str: &str) -> Result<Felt, StarknetApiError> {
    Felt::from_hex(hex::encode(ascii_str).as_str()).map_err(|_| StarknetApiError::OutOfRange {
        string: format!("The str {}, does not fit into a single felt", ascii_str),
    })
}

pub fn felt_to_u128(felt: &Felt) -> Result<u128, StarknetApiError> {
    felt.to_u128().ok_or(StarknetApiError::OutOfRange {
        string: format!("Felt {} is too big to convert to 'u128'", *felt,),
    })
}

/// A chain id.
#[derive(Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum ChainId {
    Mainnet,
    Sepolia,
    IntegrationSepolia,
    Other(String),
}

impl Serialize for ChainId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ChainId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(ChainId::from(s))
    }
}
impl From<String> for ChainId {
    fn from(s: String) -> Self {
        match s.as_ref() {
            "SN_MAIN" => ChainId::Mainnet,
            "SN_SEPOLIA" => ChainId::Sepolia,
            "SN_INTEGRATION_SEPOLIA" => ChainId::IntegrationSepolia,
            other => ChainId::Other(other.to_owned()),
        }
    }
}
impl std::fmt::Display for ChainId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChainId::Mainnet => write!(f, "SN_MAIN"),
            ChainId::Sepolia => write!(f, "SN_SEPOLIA"),
            ChainId::IntegrationSepolia => write!(f, "SN_INTEGRATION_SEPOLIA"),
            ChainId::Other(ref s) => write!(f, "{}", s),
        }
    }
}

impl ChainId {
    pub fn as_hex(&self) -> String {
        format!("0x{}", hex::encode(self.to_string()))
    }
}

pub fn deserialize_chain_id_from_hex<'de, D>(deserializer: D) -> Result<ChainId, D::Error>
where
    D: Deserializer<'de>,
{
    let hex_str = String::deserialize(deserializer)?;
    let chain_id_str =
        std::str::from_utf8(&hex::decode(hex_str.trim_start_matches("0x")).map_err(|e| {
            D::Error::custom(format!("Failed to decode the hex string {hex_str}. Error: {:?}", e))
        })?)
        .map_err(|e| {
            D::Error::custom(format!("Failed to convert to UTF-8 string. Error: {:?}", e))
        })?
        .to_string();
    Ok(ChainId::from(chain_id_str))
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
    derive_more::Display,
    Eq,
    PartialEq,
    Hash,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
    derive_more::Deref,
    SizeOf,
)]
pub struct ContractAddress(pub PatriciaKey);

impl ContractAddress {
    /// Validates the contract address is in the valid range for external access.
    /// The lower bound is above the special saved addresses and the upper bound is congruent with
    /// the storage var address upper bound.
    pub fn validate(&self) -> Result<(), StarknetApiError> {
        let value = self.0.0;
        let l2_address_upper_bound = Felt::from(*L2_ADDRESS_UPPER_BOUND);
        if (value > BLOCK_HASH_TABLE_ADDRESS.0.0) && (value < l2_address_upper_bound) {
            return Ok(());
        }

        Err(StarknetApiError::OutOfRange { string: format!("[0x2, {})", l2_address_upper_bound) })
    }
}

impl From<ContractAddress> for Felt {
    fn from(contract_address: ContractAddress) -> Felt {
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
pub const CONTRACT_ADDRESS_DOMAIN_SIZE: Felt = Felt::from_hex_unchecked(PATRICIA_KEY_UPPER_BOUND);
/// The address upper bound; it is defined to be congruent with the storage var address upper bound.
pub static L2_ADDRESS_UPPER_BOUND: LazyLock<NonZeroFelt> = LazyLock::new(|| {
    NonZeroFelt::try_from(CONTRACT_ADDRESS_DOMAIN_SIZE - Felt::from(MAX_STORAGE_ITEM_SIZE)).unwrap()
});

impl TryFrom<StarkHash> for ContractAddress {
    type Error = StarknetApiError;
    fn try_from(hash: StarkHash) -> Result<Self, Self::Error> {
        Ok(Self(PatriciaKey::try_from(hash)?))
    }
}

// TODO(Noa): Add a hash_function as a parameter
pub fn calculate_contract_address(
    salt: ContractAddressSalt,
    class_hash: ClassHash,
    constructor_calldata: &Calldata,
    deployer_address: ContractAddress,
) -> Result<ContractAddress, StarknetApiError> {
    let constructor_calldata_hash = Pedersen::hash_array(&constructor_calldata.0);
    let contract_address_prefix = format!("0x{}", hex::encode(CONTRACT_ADDRESS_PREFIX));
    let address = Pedersen::hash_array(&[
        Felt::from_hex(contract_address_prefix.as_str()).map_err(|_| {
            StarknetApiError::OutOfRange { string: contract_address_prefix.clone() }
        })?,
        *deployer_address.0.key(),
        salt.0,
        class_hash.0,
        constructor_calldata_hash,
    ]);
    let (_, address) = address.div_rem(&L2_ADDRESS_UPPER_BOUND);

    ContractAddress::try_from(address)
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
    derive_more::Display,
    derive_more::Deref,
    SizeOf,
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
    derive_more::Display,
    SizeOf,
)]
pub struct CompiledClassHash(pub StarkHash);

/// A general type for nonces.
#[derive(
    Debug,
    Default,
    derive_more::Display,
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
    SizeOf,
)]
pub struct Nonce(pub Felt);

impl Nonce {
    pub fn try_increment(&self) -> Result<Self, StarknetApiError> {
        // Check if an overflow occurred during increment.
        let incremented = self.0 + Felt::ONE;
        if incremented == Felt::ZERO {
            return Err(StarknetApiError::OutOfRange { string: format!("{:?}", self) });
        }
        Ok(Self(incremented))
    }

    pub fn try_decrement(&self) -> Result<Self, StarknetApiError> {
        // Check if an underflow occurred during decrement.
        if self.0 == Felt::ZERO {
            return Err(StarknetApiError::OutOfRange { string: format!("{:?}", self) });
        }
        Ok(Self(self.0 - Felt::ONE))
    }
}

/// The selector of an [EntryPoint](`crate::state::EntryPoint`).
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
    derive_more::Display,
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
    derive_more::Display,
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
    derive_more::Display,
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
    derive_more::Display,
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
    derive_more::Display,
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
    derive_more::Display,
    Eq,
    PartialEq,
    Default,
    Hash,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
    derive_more:: Deref,
    SizeOf,
)]
#[display(fmt = "{}", "_0.to_fixed_hex_string()")]
pub struct PatriciaKey(StarkHash);

// 2**251
pub const PATRICIA_KEY_UPPER_BOUND: &str =
    "0x800000000000000000000000000000000000000000000000000000000000000";

impl PatriciaKey {
    pub const ZERO: Self = Self(StarkHash::ZERO);
    pub const ONE: Self = Self(StarkHash::ONE);
    pub const TWO: Self = Self(StarkHash::TWO);

    pub fn key(&self) -> &StarkHash {
        &self.0
    }

    pub const fn from_hex_unchecked(val: &str) -> Self {
        Self(StarkHash::from_hex_unchecked(val))
    }
}

impl From<u128> for PatriciaKey {
    fn from(val: u128) -> Self {
        PatriciaKey::try_from(Felt::from(val)).expect("Failed to convert u128 to PatriciaKey.")
    }
}

impl_from_through_intermediate!(u128, PatriciaKey, u8, u16, u32, u64);

impl TryFrom<StarkHash> for PatriciaKey {
    type Error = StarknetApiError;

    fn try_from(value: StarkHash) -> Result<Self, Self::Error> {
        if value < CONTRACT_ADDRESS_DOMAIN_SIZE {
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
        $crate::core::PatriciaKey::try_from($crate::felt!($s)).unwrap()
    };
}

/// A utility macro to create a [`ClassHash`] from a hex string / unsigned integer representation.
#[cfg(any(feature = "testing", test))]
#[macro_export]
macro_rules! class_hash {
    ($s:expr) => {
        $crate::core::ClassHash($crate::felt!($s))
    };
}

/// A utility macro to create a [`ContractAddress`] from a hex string / unsigned integer
/// representation.
#[cfg(any(feature = "testing", test))]
#[macro_export]
macro_rules! contract_address {
    ($s:expr) => {
        $crate::core::ContractAddress($crate::patricia_key!($s))
    };
}

/// An Ethereum address.
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(try_from = "PrefixedBytesAsHex<20_usize>", into = "PrefixedBytesAsHex<20_usize>")]
pub struct EthAddress(pub H160);

impl TryFrom<Felt> for EthAddress {
    type Error = StarknetApiError;
    fn try_from(felt: Felt) -> Result<Self, Self::Error> {
        const COMPLIMENT_OF_H160: usize = std::mem::size_of::<Felt>() - H160::len_bytes();

        let bytes = felt.to_bytes_be();
        let (rest, h160_bytes) = bytes.split_at(COMPLIMENT_OF_H160);
        if rest != [0u8; COMPLIMENT_OF_H160] {
            return Err(StarknetApiError::OutOfRange { string: felt.to_string() });
        }

        Ok(EthAddress(H160::from_slice(h160_bytes)))
    }
}

impl From<EthAddress> for Felt {
    fn from(value: EthAddress) -> Self {
        Felt::from_bytes_be_slice(value.0.as_bytes())
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
#[derive(Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize)]
pub struct SequencerPublicKey(pub PublicKey);

#[derive(
    Debug, Default, Clone, Copy, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct SequencerContractAddress(pub ContractAddress);
