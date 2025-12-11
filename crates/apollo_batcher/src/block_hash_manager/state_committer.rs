#![allow(dead_code, unused_variables)]

use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;

use crate::block_hash_manager::types::{CommitmentTaskInput, CommitmentTaskOutput};

/// Commits state changes by calling the committer.
pub(crate) struct StateCommitter {
    pub(crate) tasks_receiver: Receiver<CommitmentTaskInput>,
    pub(crate) results_sender: Sender<CommitmentTaskOutput>,
    // TODO(Nimrod): Add committer client here.
}

impl StateCommitter {
    pub(crate) fn run(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self::perform_commitment_tasks(self.tasks_receiver, self.results_sender).await;
        })
    }

    pub(crate) async fn perform_commitment_tasks(
        tasks_receiver: Receiver<CommitmentTaskInput>,
        results_sender: Sender<CommitmentTaskOutput>,
    ) {
        unimplemented!()
    }
}
