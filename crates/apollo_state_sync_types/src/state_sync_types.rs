use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHeader, BlockHeaderWithoutHash, BlockNumber};
use starknet_api::block_hash::block_hash_calculator::BlockHeaderCommitments;
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionHash;

use crate::errors::StateSyncError;

pub type StateSyncResult<T> = Result<T, StateSyncError>;

/// A block that came from the state sync.
/// Contains all the data needed to update the state of the system about this block.
///
/// Blocks that came from the state sync are trusted. Therefore, SyncBlock doesn't contain data
/// needed for verifying the block
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SyncBlock {
    pub state_diff: ThinStateDiff,
    // TODO(Matan): decide if we want block hash, parent block hash and full classes here.
    pub account_transaction_hashes: Vec<TransactionHash>,
    pub l1_transaction_hashes: Vec<TransactionHash>,
    pub block_header_without_hash: BlockHeaderWithoutHash,
    pub block_header_commitments: Option<BlockHeaderCommitments>,
}

impl SyncBlock {
    pub fn get_all_transaction_hashes(&self) -> Vec<TransactionHash> {
        self.account_transaction_hashes
            .iter()
            .chain(self.l1_transaction_hashes.iter())
            .cloned()
            .collect()
    }
}

// TODO(Dean): Fill in with actual storage table names and operations.
/// Storage-related requests for state sync.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum StateSyncStorageRequest {
    /// Request to read data in Table1 for the given block height.
    Table1Replacer(BlockNumber),
}

// TODO(Dean): Fill in with actual response types matching the request variants.
/// Response for state sync storage requests.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum StateSyncStorageResponse {
    /// Table1 data for the requested operation.
    Table1Replacer(BlockHeader),
}
