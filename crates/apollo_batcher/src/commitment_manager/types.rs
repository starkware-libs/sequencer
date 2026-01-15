#![allow(dead_code)]

use std::collections::HashMap;
use std::time::Instant;

use apollo_committer_types::committer_types::{CommitBlockResponse, RevertBlockResponse};
use apollo_committer_types::communication::{CommitterRequest, CommitterRequestLabelValue};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::GlobalRoot;
use tracing::warn;

/// Input for commitment tasks.
pub(crate) struct CommitterTaskInput(pub(crate) CommitterRequest);

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
            Self::Commit(commitment_task_output) => commitment_task_output.height,
            Self::Revert(revert_task_output) => revert_task_output.height,
        }
    }

    fn task_type_to_string(&self) -> &str {
        match self {
            Self::Commit(_) => "commit",
            Self::Revert(_) => "revert",
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

pub(crate) struct TasksTiming {
    pub(crate) commit: HashMap<BlockNumber, Instant>,
    pub(crate) revert: HashMap<BlockNumber, Instant>,
}

impl TasksTiming {
    pub(crate) fn new() -> Self {
        Self { commit: HashMap::new(), revert: HashMap::new() }
    }

    pub(crate) fn start_timing(&mut self, task: CommitterRequestLabelValue, height: BlockNumber) {
        let instant = Instant::now();
        match task {
            CommitterRequestLabelValue::CommitBlock => self.commit.insert(height, instant),
            CommitterRequestLabelValue::RevertBlock => self.revert.insert(height, instant),
        };
    }

    /// Returns the duration of the task in milliseconds.
    pub(crate) fn stop_timing(&mut self, task_output: &CommitterTaskOutput) -> Option<u128> {
        let height = task_output.height();
        let map = match task_output {
            CommitterTaskOutput::Commit(_) => &mut self.commit,
            CommitterTaskOutput::Revert(_) => &mut self.revert,
        };

        let instant = map.remove(&height);
        let Some(instant) = instant else {
            let task_type = task_output.task_type_to_string();
            warn!(
                "Start time for {task_type} block number {height} not found, but stop measurement \
                 was called."
            );
            return None;
        };
        Some(instant.elapsed().as_millis())
    }
}
