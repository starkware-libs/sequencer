#[cfg(test)]
#[path = "block_test.rs"]
mod block_test;

use std::fmt::Display;
use std::ops::Deref;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use size_of::SizeOf;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::{Poseidon, StarkHash as CoreStarkHash};
use strum_macros::EnumIter;
use time::OffsetDateTime;

use crate::core::{
    ContractAddress,
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
use crate::transaction::fields::Fee;
use crate::transaction::{Transaction, TransactionHash, TransactionOutput};
use crate::StarknetApiError;

// These prices are in WEI. If we don't set them high enough the gas price when converted
// to FRI will be 0 (not allowed).
pub const TEMP_ETH_GAS_FEE_IN_WEI: u128 = u128::pow(10, 9); // 1 GWei.
pub const TEMP_ETH_BLOB_GAS_FEE_IN_WEI: u128 = u128::pow(10, 8); // 0.1 GWei.

pub const WEI_PER_ETH: u128 = u128::pow(10, 18);

/// A block.
#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct Block {
    // TODO(YoavGr): Consider renaming to BlockWithCommitments, for the header use
    // BlockHeaderWithoutHash instead of BlockHeader, and add BlockHeaderCommitments and
    // BlockHash fields.
    pub header: BlockHeader,
    pub body: BlockBody,
}

macro_rules! starknet_version_enum {
    (
        $(($variant:ident, $major:literal, $minor:literal, $patch:literal $(, $fourth:literal)?)),+,
        $latest:ident
    ) => {
        /// A version of the Starknet protocol used when creating a block.
        #[cfg_attr(any(test, feature = "testing"), derive(strum_macros::EnumIter))]
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
        pub enum StarknetVersion {
            $($variant,)+
        }

        impl StarknetVersion {
            pub const LATEST: Self = Self::$latest;
        }

        impl From<&StarknetVersion> for Vec<u8> {
            fn from(value: &StarknetVersion) -> Self {
                match value {
                    $(
                        StarknetVersion::$variant => vec![$major, $minor, $patch $(, $fourth)?],
                    )+
                }
            }
        }

        impl TryFrom<Vec<u8>> for StarknetVersion {
            type Error = StarknetApiError;

            fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
                match value[..] {
                    $(
                        [$major, $minor, $patch $(, $fourth)?] => Ok(Self::$variant),
                    )+
                    _ => Err(StarknetApiError::InvalidStarknetVersion(value)),
                }
            }
        }
    }
}

starknet_version_enum! {
    (PreV0_9_1, 0, 0, 0), // Blocks pre V0.9.1 had no starknet version field
    (V0_9_1, 0, 9, 1),
    (V0_10_0, 0, 10, 0),
    (V0_10_1, 0, 10, 1),
    (V0_10_2, 0, 10, 2),
    (V0_10_3, 0, 10, 3),
    (V0_11_0, 0, 11, 0),
    (V0_11_0_2, 0, 11, 0, 2),
    (V0_11_1, 0, 11, 1),
    (V0_11_2, 0, 11, 2),
    (V0_12_0, 0, 12, 0),
    (V0_12_1, 0, 12, 1),
    (V0_12_2, 0, 12, 2),
    (V0_12_3, 0, 12, 3),
    (V0_13_0, 0, 13, 0),
    (V0_13_1, 0, 13, 1),
    (V0_13_1_1, 0, 13, 1, 1),
    (V0_13_2, 0, 13, 2),
    (V0_13_2_1, 0, 13, 2, 1),
    (V0_13_3, 0, 13, 3),
    (V0_13_4, 0, 13, 4),
    (V0_13_5, 0, 13, 5),
    (V0_13_6, 0, 13, 6),
    (V0_14_0, 0, 14, 0),
    (V0_14_1, 0, 14, 1),
    V0_14_1
}

impl Default for StarknetVersion {
    fn default() -> Self {
        Self::LATEST
    }
}

impl From<StarknetVersion> for Vec<u8> {
    fn from(value: StarknetVersion) -> Self {
        Vec::<u8>::from(&value)
    }
}

impl std::fmt::Display for StarknetVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Vec::<u8>::from(self).iter().map(|x| x.to_string()).join("."))
    }
}

impl From<StarknetVersion> for String {
    fn from(version: StarknetVersion) -> Self {
        format!("{version}")
    }
}

impl TryFrom<String> for StarknetVersion {
    type Error = StarknetApiError;

    /// Parses a string separated by dots into a StarknetVersion.
    fn try_from(starknet_version: String) -> Result<Self, StarknetApiError> {
        let version: Vec<u8> =
            starknet_version.split('.').map(|x| x.parse::<u8>()).try_collect()?;
        Self::try_from(version)
    }
}

impl TryFrom<&str> for StarknetVersion {
    type Error = StarknetApiError;
    fn try_from(starknet_version: &str) -> Result<Self, StarknetApiError> {
        Self::try_from(starknet_version.to_string())
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
    // TODO(Gilad): Consider removing the block hash from the header (note it can be computed from
    // the rest of the fields.
    pub block_hash: BlockHash,
    pub block_header_without_hash: BlockHeaderWithoutHash,
    // The optional fields below are not included in older versions of the block.
    // Currently they are not included in any RPC spec, so we skip their serialization.
    // TODO(Yair): Once all environments support these fields, remove the Option (make sure to
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
    pub l2_gas_consumed: GasAmount,
    pub next_l2_gas_price: GasPrice,
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
    derive_more::Display,
    derive_more::Deref,
)]
pub struct BlockHash(pub StarkHash);

/// The number of a [Block](`crate::block::Block`).
#[derive(
    Debug,
    Default,
    Copy,
    derive_more::Display,
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PreviousBlockNumber(pub Option<BlockNumber>);

impl std::fmt::Display for PreviousBlockNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(block_number) => write!(f, "{}", block_number),
            None => write!(f, "None"),
        }
    }
}

impl TryFrom<Felt> for PreviousBlockNumber {
    type Error = StarknetApiError;

    /// Returns None if the Felt is Felt::MAX, which represents the previous block number for the
    /// first block.
    /// Otherwise, returns Some(BlockNumber) if the Felt is a valid block number.
    fn try_from(value: Felt) -> Result<Self, Self::Error> {
        // -1 in the Field (Felt::MAX) represents the previous block number for the first block.
        if value == Felt::MAX {
            Ok(Self(None))
        } else {
            Ok(Self(Some(BlockNumber(value.try_into().map_err(|_| {
                StarknetApiError::OutOfRange {
                    string: format!("Block number {value} is out of range"),
                }
            })?))))
        }
    }
}

impl From<PreviousBlockNumber> for Felt {
    /// Converts a [PreviousBlockNumber](`crate::block::PreviousBlockNumber`) into a Felt.
    /// Returns Felt::MAX (-1 in the field) if the previous block number is None, which means the
    /// current block is the first block.
    fn from(value: PreviousBlockNumber) -> Self {
        match value.0 {
            Some(block_number) => Self::from(block_number.0),
            None => Self::MAX,
        }
    }
}

/// A pair of a [BlockHash](`crate::block::BlockHash`) and a
/// [BlockNumber](`crate::block::BlockNumber`).
#[derive(Copy, Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockHashAndNumber {
    #[serde(rename = "block_hash")]
    pub hash: BlockHash,
    #[serde(rename = "block_number")]
    pub number: BlockNumber,
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
    SizeOf,
)]
#[serde(from = "PrefixedBytesAsHex<16_usize>", into = "PrefixedBytesAsHex<16_usize>")]
pub struct GasPrice(pub u128);

impl GasPrice {
    pub fn wei_to_fri(self, eth_to_fri_rate: u128) -> Result<GasPrice, StarknetApiError> {
        // We use integer division since wei * eth_to_fri_rate is expected to be high enough to not
        // cause too much precision loss.
        Ok(self
            .checked_mul_u128(eth_to_fri_rate)
            .ok_or_else(|| {
                StarknetApiError::GasPriceConversionError("Gas price is too high".to_string())
            })?
            .checked_div(WEI_PER_ETH)
            .expect("WEI_PER_ETH must be non-zero"))
    }
    pub fn fri_to_wei(self, eth_to_fri_rate: u128) -> Result<GasPrice, StarknetApiError> {
        self.checked_mul_u128(WEI_PER_ETH)
            .ok_or_else(|| {
                StarknetApiError::GasPriceConversionError("Gas price is too high".to_string())
            })?
            .checked_div(eth_to_fri_rate)
            .ok_or_else(|| {
                StarknetApiError::GasPriceConversionError(
                    "FRI to ETH rate must be non-zero".to_string(),
                )
            })
    }
}

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

    pub fn saturating_add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }

    pub fn checked_mul(self, rhs: GasAmount) -> Option<Fee> {
        self.0.checked_mul(u128::from(rhs.0)).map(Fee)
    }

    pub fn checked_mul_u128(self, rhs: u128) -> Option<GasPrice> {
        self.0.checked_mul(rhs).map(Self)
    }

    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        self.0.checked_add(rhs.0).map(Self)
    }

    pub fn checked_div(self, rhs: u128) -> Option<Self> {
        self.0.checked_div(rhs).map(Self)
    }
}

/// Utility struct representing a non-zero gas price. Useful when a gas amount must be computed by
/// taking a fee amount and dividing by the gas price.
#[derive(
    Copy, Clone, Debug, Deserialize, Eq, PartialEq, Serialize, PartialOrd, Ord, derive_more::Display,
)]
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

    pub fn checked_add(self, rhs: GasPrice) -> Option<Self> {
        self.get()
            .checked_add(rhs)
            .map(|x| Self::new(x).expect("Add GasPrice should not result in zero"))
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

// TODO(Arni): Remove derive of Default. Gas prices should always be set.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct GasPriceVector {
    pub l1_gas_price: NonzeroGasPrice,
    pub l1_data_gas_price: NonzeroGasPrice,
    pub l2_gas_price: NonzeroGasPrice,
}

#[derive(Clone, Copy, Hash, EnumIter, Eq, PartialEq)]
pub enum FeeType {
    Strk,
    Eth,
}

// TODO(Arni): Remove derive of Default. Gas prices should always be set.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct GasPrices {
    pub eth_gas_prices: GasPriceVector,  // In wei.
    pub strk_gas_prices: GasPriceVector, // In fri.
}

impl GasPrices {
    pub fn l1_gas_price(&self, fee_type: &FeeType) -> NonzeroGasPrice {
        self.gas_price_vector(fee_type).l1_gas_price
    }

    pub fn l1_data_gas_price(&self, fee_type: &FeeType) -> NonzeroGasPrice {
        self.gas_price_vector(fee_type).l1_data_gas_price
    }

    pub fn l2_gas_price(&self, fee_type: &FeeType) -> NonzeroGasPrice {
        self.gas_price_vector(fee_type).l2_gas_price
    }

    pub fn gas_price_vector(&self, fee_type: &FeeType) -> &GasPriceVector {
        match fee_type {
            FeeType::Strk => &self.strk_gas_prices,
            FeeType::Eth => &self.eth_gas_prices,
        }
    }
}

// TODO(Arni): replace all relevant instances of `u64` with UnixTimestamp.
/// A Unix timestamp in seconds since the Unix epoch (January 1, 1970).
pub type UnixTimestamp = u64;

/// The timestamp of a [Block](`crate::block::Block`).
#[derive(
    Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
pub struct BlockTimestamp(pub UnixTimestamp);

impl BlockTimestamp {
    pub fn saturating_add(self, rhs: &u64) -> Self {
        Self(self.0.saturating_add(*rhs))
    }

    pub fn saturating_sub(self, rhs: &u64) -> Self {
        Self(self.0.saturating_sub(*rhs))
    }
}

impl Deref for BlockTimestamp {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<u64> for BlockTimestamp {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl Display for BlockTimestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let seconds_from_epoch = i64::try_from(self.0).map_err(|_| std::fmt::Error)?;
        let time_in_range =
            OffsetDateTime::from_unix_timestamp(seconds_from_epoch).map_err(|_| std::fmt::Error)?;
        write!(f, "{time_in_range}")
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BlockInfo {
    pub block_number: BlockNumber,
    pub block_timestamp: BlockTimestamp,

    // Fee-related.
    pub sequencer_address: ContractAddress,
    pub gas_prices: GasPrices,
    pub use_kzg_da: bool,
}

/// The signature of a [Block](`crate::block::Block`), signed by the sequencer. The signed message
/// is defined as poseidon_hash(block_hash, state_diff_commitment).
#[derive(Debug, Default, Copy, Clone, Eq, PartialEq, Hash, Deserialize, Serialize)]
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
