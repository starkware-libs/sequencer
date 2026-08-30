use std::collections::HashMap;
use std::fmt::Display;
use std::time::Instant;

use apollo_committer_types::committer_types::{
    CommitBlockRequest,
    CommitBlockResponse,
    ReadPathsAndCommitBlockRequest,
    RevertBlockRequest,
    RevertBlockResponse,
};
use apollo_committer_types::communication::CommitterRequestLabelValue;
use apollo_storage::state_commitment_infos::CompressedStateCommitmentInfos;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::GlobalRoot;
use tracing::warn;

/// Input for commitment tasks.
#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
pub(crate) enum CommitterTaskInput {
    Commit(CommitBlockRequest),
    ReadPathsAndCommitBlock(ReadPathsAndCommitBlockRequest),
    Revert(RevertBlockRequest),
}

impl CommitterTaskInput {
    pub(crate) fn height(&self) -> BlockNumber {
        match self {
            Self::Commit(request) => request.height,
            Self::ReadPathsAndCommitBlock(request) => request.commit.height,
            Self::Revert(request) => request.height,
        }
    }

    /// The committer endpoint this task will use.
    pub(crate) fn task_type(&self) -> CommitterRequestLabelValue {
        match self {
            Self::Commit(_) => CommitterRequestLabelValue::CommitBlock,
            Self::ReadPathsAndCommitBlock(_) => CommitterRequestLabelValue::ReadPathsAndCommitBlock,
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
            Self::ReadPathsAndCommitBlock(request) => write!(
                f,
                "ReadPathsAndCommitBlock(height={}, state_diff_commitment={:?}, \
                 num_accessed_keys={})",
                request.commit.height,
                request.commit.state_diff_commitment,
                request.accessed_keys.len()
            ),
            Self::Revert(request) => write!(f, "Revert(height={})", request.height),
        }
    }
}

#[derive(Clone)]
pub(crate) struct CommitmentTaskOutput {
    pub(crate) response: CommitBlockResponse,
    pub(crate) height: BlockNumber,
    // Compressed commitment infos from the committer. `None` when the block was committed via
    // `CommitBlock` (no accessed keys to request the Patricia witnesses).
    pub(crate) state_commitment_infos: Option<CompressedStateCommitmentInfos>,
}

#[derive(Clone, Debug)]
pub(crate) struct RevertTaskOutput {
    pub(crate) response: RevertBlockResponse,
    pub(crate) height: BlockNumber,
}

#[derive(Clone)]
pub(crate) enum CommitterTaskOutput {
    Commit(CommitmentTaskOutput),
    ReadPathsAndCommitBlock(CommitmentTaskOutput),
    Revert(RevertTaskOutput),
}

impl CommitterTaskOutput {
    pub(crate) fn expect_commitment(self) -> CommitmentTaskOutput {
        match self {
            Self::Commit(commitment_task_output) => commitment_task_output,
            Self::ReadPathsAndCommitBlock(commitment_task_output) => commitment_task_output,
            Self::Revert(revert_task_output) => {
                panic!("Got revert output for height {}.", revert_task_output.height)
            }
        }
    }

    pub(crate) fn height(&self) -> BlockNumber {
        match self {
            Self::Commit(output) => output.height,
            Self::ReadPathsAndCommitBlock(output) => output.height,
            Self::Revert(output) => output.height,
        }
    }

    pub(crate) fn task_label(&self) -> CommitterRequestLabelValue {
        match self {
            Self::Commit(_) => CommitterRequestLabelValue::CommitBlock,
            Self::ReadPathsAndCommitBlock(_) => CommitterRequestLabelValue::ReadPathsAndCommitBlock,
            Self::Revert(_) => CommitterRequestLabelValue::RevertBlock,
        }
    }
}

pub(crate) struct FinalBlockCommitment {
    pub(crate) height: BlockNumber,
    // Field is optional because for old blocks there are no component hashes, so the block hash
    // cannot be finalized.
    pub(crate) block_hash: Option<BlockHash>,
    pub(crate) global_root: GlobalRoot,
    // Compressed commitment infos from the committer. `None` when the block was committed via
    // `CommitBlock` (no accessed keys to request the Patricia witnesses).
    pub(crate) state_commitment_infos: Option<CompressedStateCommitmentInfos>,
}

pub(crate) struct TaskTimer {
    pub(crate) commit: HashMap<BlockNumber, Instant>,
    pub(crate) read_paths_and_commit_block: HashMap<BlockNumber, Instant>,
    pub(crate) revert: HashMap<BlockNumber, Instant>,
}

impl TaskTimer {
    pub(crate) fn new() -> Self {
        Self {
            commit: HashMap::new(),
            read_paths_and_commit_block: HashMap::new(),
            revert: HashMap::new(),
        }
    }

    /// Returns the timer map for the given task label.
    fn map_for_label(
        &mut self,
        task: CommitterRequestLabelValue,
    ) -> &mut HashMap<BlockNumber, Instant> {
        match task {
            CommitterRequestLabelValue::CommitBlock => &mut self.commit,
            CommitterRequestLabelValue::ReadPathsAndCommitBlock => {
                &mut self.read_paths_and_commit_block
            }
            CommitterRequestLabelValue::RevertBlock => &mut self.revert,
        }
    }

    pub(crate) fn start_timer(&mut self, task: CommitterRequestLabelValue, height: BlockNumber) {
        self.map_for_label(task).insert(height, Instant::now());
    }

    /// Returns the duration of the task in milliseconds.
    pub(crate) fn stop_timer(
        &mut self,
        task: CommitterRequestLabelValue,
        height: BlockNumber,
    ) -> Option<u64> {
        let Some(instant) = self.map_for_label(task).remove(&height) else {
            warn!(
                "Can't stop timer for {task:?} task for block number {height} because timer was \
                 never started."
            );
            return None;
        };
        let duration = instant.elapsed().as_millis();
        Some(u64::try_from(duration).expect("Duration is not more than 500 million years."))
    }
}
