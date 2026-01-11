#![allow(dead_code)]

use std::fmt::Display;

use apollo_committer_types::committer_types::{
    CommitBlockRequest,
    CommitBlockResponse,
    RevertBlockRequest,
    RevertBlockResponse,
};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::GlobalRoot;

/// Input for commitment tasks.
pub(crate) enum CommitterTaskInput {
    Commit(CommitBlockRequest),
    Revert(RevertBlockRequest),
}

impl CommitterTaskInput {
    pub(crate) fn height(&self) -> BlockNumber {
        match self {
            Self::Commit(request) => request.height,
            Self::Revert(request) => request.height,
        }
    }
}

impl Display for CommitterTaskInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Commit(request) => write!(
                f,
                "Commit(height={}, state_diff_commitment={:?})",
                request.height, request.state_diff_commitment
            ),
            Self::Revert(request) => write!(f, "Revert(height={})", request.height),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CommitmentTaskOutput {
    pub(crate) response: CommitBlockResponse,
    pub(crate) height: BlockNumber,
}

#[derive(Clone, Debug)]
pub(crate) struct RevertTaskOutput {
    pub(crate) response: RevertBlockResponse,
    pub(crate) height: BlockNumber,
}

#[derive(Clone, Debug)]
pub(crate) enum CommitterTaskOutput {
    Commit(CommitmentTaskOutput),
    Revert(RevertTaskOutput),
}

impl CommitterTaskOutput {
    pub(crate) fn expect_commitment(self) -> CommitmentTaskOutput {
        match self {
            Self::Commit(commitment_task_output) => commitment_task_output,
            Self::Revert(_) => panic!("Got revert output: {self:?}"),
        }
    }

    pub(crate) fn height(&self) -> BlockNumber {
        match self {
            Self::Commit(CommitmentTaskOutput { height, .. })
            | Self::Revert(RevertTaskOutput { height, .. }) => *height,
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
