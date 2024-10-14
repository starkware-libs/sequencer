#[cfg(test)]
#[path = "block_test.rs"]
mod block_test;

use std::fmt::Display;

use derive_more::Display;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::{Poseidon, StarkHash as CoreStarkHash};

use crate::core::{
    EventCommitment,
    GlobalRoot,
    ReceiptCommitment,
    SequencerContractAddress,
    SequencerPublicKey,
    StateDiffCommitment,
    TransactionCommitment,
};
use crate::crypto::utils::{verify_message_hash_signature, CryptoError, Signature};
use crate::data_availability::L1DataAvailabilityMode;
use crate::execution_resources::GasAmount;
use crate::hash::StarkHash;
use crate::serde_utils::{BytesAsHex, PrefixedBytesAsHex};
use crate::transaction::fields::{Fee, TransactionHash};
use crate::transaction::{Transaction, TransactionOutput};
use crate::StarknetApiError;

/// A block.
#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct Block {
    // TODO: Consider renaming to BlockWithCommitments, for the header use BlockHeaderWithoutHash
    // instead of BlockHeader, and add BlockHeaderCommitments and BlockHash fields.
    pub header: BlockHeader,
    pub body: BlockBody,
}

/// A version of the Starknet protocol used when creating a block.
#[derive(Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct StarknetVersion(pub Vec<u8>);

impl Default for StarknetVersion {
    fn default() -> Self {
        Self(vec![0, 0, 0])
    }
}

impl Display for StarknetVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.iter().map(|x| x.to_string()).join("."))
    }
}

impl TryFrom<String> for StarknetVersion {
    type Error = StarknetApiError;

    /// Parses a string separated by dots into a StarknetVersion.
    fn try_from(starknet_version: String) -> Result<Self, StarknetApiError> {
        Ok(Self(starknet_version.split('.').map(|x| x.parse::<u8>()).try_collect()?))
    }
}

impl Serialize for StarknetVersion {
    fn serialize<S>(
        &self,
        serializer: S,
    ) -> Result<<S as serde::Serializer>::Ok, <S as serde::Serializer>::Error>
    where
        S: serde::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for StarknetVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as serde::Deserializer<'de>>::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let version = String::deserialize(deserializer)?;
        StarknetVersion::try_from(version).map_err(serde::de::Error::custom)
    }
}

/// The header of a [Block](`crate::block::Block`).
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct BlockHeader {
    // TODO: Consider removing the block hash from the header (note it can be computed from
    // the rest of the fields.
    pub block_hash: BlockHash,
    pub block_header_without_hash: BlockHeaderWithoutHash,
    // The optional fields below are not included in older versions of the block.
    // Currently they are not included in any RPC spec, so we skip their serialization.
    // TODO: Once all environments support these fields, remove the Option (make sure to
    // update/resync any storage is missing the data).
    #[serde(skip_serializing)]
    pub state_diff_commitment: Option<StateDiffCommitment>,
    #[serde(skip_serializing)]
    pub state_diff_length: Option<usize>,
    #[serde(skip_serializing)]
    pub transaction_commitment: Option<TransactionCommitment>,
    #[serde(skip_serializing)]
    pub event_commitment: Option<EventCommitment>,
    #[serde(skip_serializing)]
    pub n_transactions: usize,
    #[serde(skip_serializing)]
    pub n_events: usize,
    #[serde(skip_serializing)]
    pub receipt_commitment: Option<ReceiptCommitment>,
}

/// The header of a [Block](`crate::block::Block`) without hashing.
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct BlockHeaderWithoutHash {
    pub parent_hash: BlockHash,
    pub block_number: BlockNumber,
    pub l1_gas_price: GasPricePerToken,
    pub l1_data_gas_price: GasPricePerToken,
    pub l2_gas_price: GasPricePerToken,
    pub state_root: GlobalRoot,
    pub sequencer: SequencerContractAddress,
    pub timestamp: BlockTimestamp,
    pub l1_da_mode: L1DataAvailabilityMode,
    pub starknet_version: StarknetVersion,
}

/// The [transactions](`crate::transaction::Transaction`) and their
/// [outputs](`crate::transaction::TransactionOutput`) in a [block](`crate::block::Block`).
#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct BlockBody {
    pub transactions: Vec<Transaction>,
    pub transaction_outputs: Vec<TransactionOutput>,
    pub transaction_hashes: Vec<TransactionHash>,
}

/// The status of a [Block](`crate::block::Block`).
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum BlockStatus {
    /// A pending block; i.e., a block that is yet to be closed.
    #[serde(rename = "PENDING")]
    Pending,
    /// A block that was created on L2.
    #[serde(rename = "ACCEPTED_ON_L2")]
    AcceptedOnL2,
    /// A block that was accepted on L1.
    #[serde(rename = "ACCEPTED_ON_L1")]
    AcceptedOnL1,
    /// A block rejected on L1.
    #[serde(rename = "REJECTED")]
    Rejected,
}

/// The hash of a [Block](`crate::block::Block`).
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
pub struct BlockHash(pub StarkHash);

/// The number of a [Block](`crate::block::Block`).
#[derive(
    Debug,
    Default,
    Copy,
    Display,
    Clone,
    Eq,
    PartialEq,
    Hash,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
)]
pub struct BlockNumber(pub u64);

impl BlockNumber {
    /// Returns the next block number, without checking if it's in range.
    pub fn unchecked_next(&self) -> BlockNumber {
        BlockNumber(self.0 + 1)
    }

    /// Returns the next block number, or None if the next block number is out of range.
    pub fn next(&self) -> Option<Self> {
        Some(Self(self.0.checked_add(1)?))
    }

    /// Returns the previous block number, or None if the previous block number is out of range.
    pub fn prev(&self) -> Option<BlockNumber> {
        match self.0 {
            0 => None,
            i => Some(BlockNumber(i - 1)),
        }
    }

    /// Returns an iterator over the block numbers from self to up_to (exclusive).
    pub fn iter_up_to(&self, up_to: Self) -> impl Iterator<Item = BlockNumber> {
        let range = self.0..up_to.0;
        range.map(Self)
    }
}

// TODO(yair): Consider moving GasPricePerToken and GasPrice to core.
/// The gas price per token.
#[derive(
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct GasPricePerToken {
    pub price_in_fri: GasPrice,
    pub price_in_wei: GasPrice,
}

/// The gas price at a [Block](`crate::block::Block`).
#[derive(
    Debug,
    Copy,
    Clone,
    Default,
    derive_more::Display,
    Eq,
    PartialEq,
    Hash,
    Deserialize,
    Serialize,
    PartialOrd,
    Ord,
)]
#[serde(from = "PrefixedBytesAsHex<16_usize>", into = "PrefixedBytesAsHex<16_usize>")]
pub struct GasPrice(pub u128);

macro_rules! impl_from_uint_for_gas_price {
    ($($uint:ty),*) => {
        $(
            impl From<$uint> for GasPrice {
                fn from(val: $uint) -> Self {
                    GasPrice(u128::from(val))
                }
            }
        )*
    };
}

impl_from_uint_for_gas_price!(u8, u16, u32, u64, u128);

impl From<PrefixedBytesAsHex<16_usize>> for GasPrice {
    fn from(val: PrefixedBytesAsHex<16_usize>) -> Self {
        u128::from_be_bytes(val.0).into()
    }
}

impl From<GasPrice> for PrefixedBytesAsHex<16_usize> {
    fn from(val: GasPrice) -> Self {
        BytesAsHex(val.0.to_be_bytes())
    }
}

impl From<GasPrice> for Felt {
    fn from(val: GasPrice) -> Self {
        Felt::from(val.0)
    }
}

impl GasPrice {
    pub const fn saturating_mul(self, rhs: GasAmount) -> Fee {
        #[allow(clippy::as_conversions)]
        Fee(self.0.saturating_mul(rhs.0 as u128))
    }

    pub fn checked_mul(self, rhs: GasAmount) -> Option<Fee> {
        self.0.checked_mul(u128::from(rhs.0)).map(Fee)
    }
}

/// Utility struct representing a non-zero gas price. Useful when a gas amount must be computed by
/// taking a fee amount and dividing by the gas price.
#[derive(Copy, Clone, Debug, derive_more::Display)]
pub struct NonzeroGasPrice(GasPrice);

impl NonzeroGasPrice {
    pub const MIN: Self = Self(GasPrice(1));

    pub fn new(price: GasPrice) -> Result<Self, StarknetApiError> {
        if price.0 == 0 {
            return Err(StarknetApiError::ZeroGasPrice);
        }
        Ok(Self(price))
    }

    pub const fn get(&self) -> GasPrice {
        self.0
    }

    pub const fn saturating_mul(self, rhs: GasAmount) -> Fee {
        self.get().saturating_mul(rhs)
    }

    pub fn checked_mul(self, rhs: GasAmount) -> Option<Fee> {
        self.get().checked_mul(rhs)
    }

    #[cfg(any(test, feature = "testing"))]
    pub const fn new_unchecked(price: GasPrice) -> Self {
        Self(price)
    }
}

impl Default for NonzeroGasPrice {
    fn default() -> Self {
        Self::MIN
    }
}

impl From<NonzeroGasPrice> for GasPrice {
    fn from(val: NonzeroGasPrice) -> Self {
        val.0
    }
}

impl TryFrom<GasPrice> for NonzeroGasPrice {
    type Error = StarknetApiError;

    fn try_from(price: GasPrice) -> Result<Self, Self::Error> {
        NonzeroGasPrice::new(price)
    }
}

macro_rules! impl_try_from_uint_for_nonzero_gas_price {
    ($($uint:ty),*) => {
        $(
            impl TryFrom<$uint> for NonzeroGasPrice {
                type Error = StarknetApiError;

                fn try_from(val: $uint) -> Result<Self, Self::Error> {
                    NonzeroGasPrice::new(GasPrice::from(val))
                }
            }
        )*
    };
}

impl_try_from_uint_for_nonzero_gas_price!(u8, u16, u32, u64, u128);

#[derive(Clone, Debug)]
pub struct GasPriceVector {
    pub l1_gas_price: NonzeroGasPrice,
    pub l1_data_gas_price: NonzeroGasPrice,
    pub l2_gas_price: NonzeroGasPrice,
}

/// The timestamp of a [Block](`crate::block::Block`).
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockTimestamp(pub u64);

/// The signature of a [Block](`crate::block::Block`), signed by the sequencer. The signed message
/// is defined as poseidon_hash(block_hash, state_diff_commitment).
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockSignature(pub Signature);

/// The error type returned from the block verification functions.
#[derive(thiserror::Error, Clone, Debug)]
pub enum BlockVerificationError {
    #[error("Failed to verify the signature of block {block_hash}. Error: {error}")]
    BlockSignatureVerificationFailed { block_hash: BlockHash, error: CryptoError },
}

/// Verifies that the the block header was signed by the expected sequencer.
pub fn verify_block_signature(
    sequencer_pub_key: &SequencerPublicKey,
    signature: &BlockSignature,
    state_diff_commitment: &GlobalRoot,
    block_hash: &BlockHash,
) -> Result<bool, BlockVerificationError> {
    let message_hash = Poseidon::hash_array(&[block_hash.0, state_diff_commitment.0]);
    verify_message_hash_signature(&message_hash, &signature.0, &sequencer_pub_key.0).map_err(
        |err| BlockVerificationError::BlockSignatureVerificationFailed {
            block_hash: *block_hash,
            error: err,
        },
    )
}
