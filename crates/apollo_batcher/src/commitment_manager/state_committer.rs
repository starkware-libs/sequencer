#![allow(dead_code, unused_variables, unused_mut)]

use apollo_committer_types::communication::SharedCommitterClient;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;

use crate::commitment_manager::types::{CommitmentTaskInput, CommitmentTaskOutput};

/// Commits state changes by calling the committer.
pub(crate) struct StateCommitter {
    pub(crate) tasks_receiver: Receiver<CommitmentTaskInput>,
    pub(crate) results_sender: Sender<CommitmentTaskOutput>,
    pub(crate) committer_client: SharedCommitterClient,
}

impl StateCommitter {
    pub(crate) fn run(self) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self::perform_commitment_tasks(self.tasks_receiver, self.results_sender).await;
        })
    }

    pub(crate) async fn perform_commitment_tasks(
        mut tasks_receiver: Receiver<CommitmentTaskInput>,
        mut results_sender: Sender<CommitmentTaskOutput>,
    ) {
        // Placeholder: simply drain the receiver and do nothing.
        // TODO(Amos): Implement the actual commitment tasks logic.
        while let Some(_task) = tasks_receiver.recv().await {}
    }
}
