#[cfg(test)]
#[path = "block_test.rs"]
mod block_test;

use std::fmt::Display;

use derive_more::Display;
use serde::{Deserialize, Serialize};

use crate::core::{
    EventCommitment, GlobalRoot, ReceiptCommitment, SequencerContractAddress, SequencerPublicKey,
    StateDiffCommitment, TransactionCommitment,
};
use crate::crypto::{verify_message_hash_signature, CryptoError, Signature};
use crate::data_availability::L1DataAvailabilityMode;
use crate::hash::{poseidon_hash_array, StarkHash};
use crate::serde_utils::{BytesAsHex, PrefixedBytesAsHex};
use crate::transaction::{Transaction, TransactionHash, TransactionOutput};

/// A block.
#[derive(Debug, Default, Clone, Eq, PartialEq, Deserialize, Serialize)]
pub struct Block {
    pub header: BlockHeader,
    pub body: BlockBody,
}

/// A version of the Starknet protocol used when creating a block.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct StarknetVersion(pub String);

impl Default for StarknetVersion {
    fn default() -> Self {
        Self("0.0.0".to_string())
    }
}

impl Display for StarknetVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The header of a [Block](`crate::block::Block`).
#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct BlockHeader {
    // TODO: Consider removing the block hash from the header (note it can be computed from
    // the rest of the fields.
    pub block_hash: BlockHash,
    pub parent_hash: BlockHash,
    pub block_number: BlockNumber,
    pub l1_gas_price: GasPricePerToken,
    pub l1_data_gas_price: GasPricePerToken,
    pub state_root: GlobalRoot,
    pub sequencer: SequencerContractAddress,
    pub timestamp: BlockTimestamp,
    pub l1_da_mode: L1DataAvailabilityMode,
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
    pub n_transactions: Option<usize>,
    #[serde(skip_serializing)]
    pub n_events: Option<usize>,
    #[serde(skip_serializing)]
    pub receipt_commitment: Option<ReceiptCommitment>,
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
    Debug, Copy, Clone, Default, Eq, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord,
)]
#[serde(from = "PrefixedBytesAsHex<16_usize>", into = "PrefixedBytesAsHex<16_usize>")]
pub struct GasPrice(pub u128);

impl From<PrefixedBytesAsHex<16_usize>> for GasPrice {
    fn from(val: PrefixedBytesAsHex<16_usize>) -> Self {
        GasPrice(u128::from_be_bytes(val.0))
    }
}

impl From<GasPrice> for PrefixedBytesAsHex<16_usize> {
    fn from(val: GasPrice) -> Self {
        BytesAsHex(val.0.to_be_bytes())
    }
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
    let message_hash = poseidon_hash_array(&[block_hash.0, state_diff_commitment.0]);
    verify_message_hash_signature(&message_hash.0, &signature.0, &sequencer_pub_key.0).map_err(
        |err| BlockVerificationError::BlockSignatureVerificationFailed {
            block_hash: *block_hash,
            error: err,
        },
    )
}
