#![allow(dead_code)]
use apollo_committer_types::communication::{CommitterClientResponse, CommitterRequest};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::GlobalRoot;

/// Input for commitment tasks.
pub(crate) struct CommitmentTaskInput(pub(crate) CommitterRequest);

/// Output of commitment tasks.
pub(crate) struct CommitmentTaskOutput {
    pub(crate) committer_response: CommitterClientResponse,
    pub(crate) height: BlockNumber,
}

pub(crate) struct FinalBlockCommitment {
    pub(crate) height: BlockNumber,
    // Field is optional because for old blocks there are no component hashes, so the block hash
    // cannot be finalized.
    pub(crate) block_hash: Option<BlockHash>,
    pub(crate) global_root: GlobalRoot,
}
