use serde::{Deserialize, Serialize};
use starknet_api::block::BlockHash;
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionHash;

use crate::errors::StateSyncError;

pub type StateSyncResult<T> = Result<T, StateSyncError>;

/// A block that came from state sync.
/// Contains all the data needed to update the state of the system about this block.
///
/// We trust blocks that came from sync. Therefore, SyncBlock doesn't contain data needed for
/// verifying the block
pub struct SyncBlock {
    pub block_hash: BlockHash,
    pub parent_block_hash: BlockHash,
    pub state_diff: ThinStateDiff,
    // TODO: decide if we want full classes here.
    pub transaction_hashes: Vec<TransactionHash>,
}
