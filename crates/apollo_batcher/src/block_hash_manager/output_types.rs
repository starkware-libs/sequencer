#![allow(dead_code)]
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::GlobalRoot;

/// Output of commitment tasks.
pub(crate) enum CommitmentTaskOutput {
    BlockHashCalculation(BlockHashCalculationOutput),
    TrustedBlockHash(TrustedBlockHashOutput),
}

/// Output of block hash calculation task.
pub(crate) struct BlockHashCalculationOutput {
    pub(crate) global_root: GlobalRoot,
    pub(crate) height: BlockNumber,
}

/// Output of syncing the committer with a trusted block hash.
pub(crate) struct TrustedBlockHashOutput {
    pub(crate) global_root: GlobalRoot,
    pub(crate) height: BlockNumber,
    pub(crate) parent_hash: BlockHash,
}
