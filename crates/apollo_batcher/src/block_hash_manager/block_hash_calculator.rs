#![allow(dead_code)]

use tokio::sync::mpsc::{Receiver, Sender};

use crate::block_hash_manager::input_types::CommitmentTaskInput;
use crate::block_hash_manager::output_types::CommitmentTaskOutput;

/// Commits state changes by calling the committer.
pub(crate) struct StateCommitter {
    pub(crate) tasks_receiver: Receiver<CommitmentTaskInput>,
    pub(crate) results_sender: Sender<CommitmentTaskOutput>,
    // TODO(Nimrod): Add committer client here.
}
