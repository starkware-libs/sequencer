#![allow(dead_code)]
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::{GlobalRoot, StateDiffCommitment};
use starknet_api::state::ThinStateDiff;

/// Input for commitment tasks.
pub(crate) struct CommitmentTaskInput {
    pub(crate) state_diff: ThinStateDiff,
    pub(crate) height: BlockNumber,
    // Field is optional because for old blocks, the state diff commitment might not be available.
    pub(crate) state_diff_commitment: Option<StateDiffCommitment>,
}

/// Output of commitment tasks.
pub(crate) struct CommitmentTaskOutput {
    pub(crate) global_root: GlobalRoot,
    pub(crate) height: BlockNumber,
}

pub(crate) struct FinalBlockCommitment {
    pub(crate) height: BlockNumber,
    // Field is optional because for old blocks, the block hash is learnable from sync.
    pub(crate) block_hash: Option<BlockHash>,
    pub(crate) global_root: GlobalRoot,
}
