#![allow(dead_code)]
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::block_hash::block_hash_calculator::PartialBlockHashComponents;
use starknet_api::state::ThinStateDiff;

/// Input for commitment tasks.
pub(crate) enum CommitmentTaskInput {
    BlockHashCalculation(BlockHashCalculationInput),
    TrustedBlockHash(TrustedBlockHashInput),
}

/// Input for block hash calculation task. This task also syncs the committer.
pub(crate) struct BlockHashCalculationInput {
    pub(crate) state_diff: ThinStateDiff,
    pub(crate) height: BlockNumber,
    pub(crate) partial_components: PartialBlockHashComponents,
    // This is optional because we want to verify the parent hash of blocks coming from sync.
    pub(crate) optional_parent_hash: Option<BlockHash>,
}

/// Input for syncing the committer and trust the given block hash.
pub(crate) struct TrustedBlockHashInput {
    pub(crate) state_diff: ThinStateDiff,
    pub(crate) height: BlockNumber,
    pub(crate) parent_hash: BlockHash,
}
