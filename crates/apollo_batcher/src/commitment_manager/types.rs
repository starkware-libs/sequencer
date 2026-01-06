#![allow(dead_code)]

use apollo_committer_types::committer_types::{CommitBlockResponse, RevertBlockResponse};
use apollo_committer_types::errors::CommitterClientResult;
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

#[derive(Clone)]
pub(crate) struct CommitmentTaskOutput {
    pub(crate) response: CommitBlockResponse,
    pub(crate) height: BlockNumber,
}

#[derive(Clone)]
pub(crate) struct RevertTaskOutput {
    pub(crate) response: RevertBlockResponse,
    pub(crate) height: BlockNumber,
}

pub type CommitmentTaskResult = CommitterClientResult<CommitmentTaskOutput>;
pub type RevertTaskResult = CommitterClientResult<RevertTaskOutput>;

#[derive(Clone)]
pub(crate) enum CommitterTaskResult {
    Commit(CommitmentTaskResult),
    Revert(RevertTaskResult),
}

impl CommitterTaskResult {
    pub(crate) fn expect_commitment(self) -> CommitmentTaskResult {
        match self {
            Self::Commit(commitment_task_result) => commitment_task_result,
            Self::Revert(_) => panic!("Get revert result at unwrapping commitment."),
        }
    }
}

pub(crate) struct FinalBlockCommitment {
    pub(crate) height: BlockNumber,
    // Field is optional because for old blocks there are no component hashes, so the block hash
    // cannot be finalized.
    pub(crate) block_hash: Option<BlockHash>,
    pub(crate) global_root: GlobalRoot,
}
