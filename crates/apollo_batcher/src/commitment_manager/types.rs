#![allow(dead_code)]

use std::collections::HashMap;
use std::fmt::Display;
use std::time::Instant;

use apollo_committer_types::committer_types::{
    CommitBlockRequest,
    CommitBlockResponse,
    RevertBlockRequest,
    RevertBlockResponse,
};
use apollo_committer_types::communication::CommitterRequestLabelValue;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::GlobalRoot;
use tracing::warn;

/// Input for commitment tasks.
#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
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

    pub(crate) fn task_type(&self) -> CommitterRequestLabelValue {
        match self {
            Self::Commit(_) => CommitterRequestLabelValue::CommitBlock,
            Self::Revert(_) => CommitterRequestLabelValue::RevertBlock,
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

pub(crate) struct TaskTimer {
    pub(crate) commit: HashMap<BlockNumber, Instant>,
    pub(crate) revert: HashMap<BlockNumber, Instant>,
}

impl TaskTimer {
    pub(crate) fn new() -> Self {
        Self { commit: HashMap::new(), revert: HashMap::new() }
    }

    pub(crate) fn start_timer(&mut self, task: CommitterRequestLabelValue, height: BlockNumber) {
        let instant = Instant::now();
        match task {
            CommitterRequestLabelValue::CommitBlock => self.commit.insert(height, instant),
            CommitterRequestLabelValue::RevertBlock => self.revert.insert(height, instant),
        };
    }

    /// Returns the duration of the task in seconds.
    pub(crate) fn stop_timer(
        &mut self,
        task: CommitterRequestLabelValue,
        height: BlockNumber,
    ) -> Option<f64> {
        let map = match task {
            CommitterRequestLabelValue::CommitBlock => &mut self.commit,
            CommitterRequestLabelValue::RevertBlock => &mut self.revert,
        };

        let instant = map.remove(&height);
        let Some(instant) = instant else {
            warn!(
                "Can't stop timer for {task:?} task for block number {height} because timer was \
                 never started."
            );
            return None;
        };
        Some(instant.elapsed().as_secs_f64())
    }
}
