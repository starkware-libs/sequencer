#![allow(dead_code, unused_variables, unused_mut)]

use apollo_committer_types::communication::SharedCommitterClient;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;

use crate::commitment_manager::types::{CommitmentTaskInput, CommitmentTaskOutput};

/// Commits state changes by calling the committer.
pub(crate) trait StateCommitterTrait {
    /// Creates a new instance and starts thread which performs commitment tasks.
    fn create(
        tasks_receiver: Receiver<CommitmentTaskInput>,
        results_sender: Sender<CommitmentTaskOutput>,
        committer_client: SharedCommitterClient,
    ) -> Self;
    /// Returns a handle to the thread performing commitment tasks.
    fn get_handle(&self) -> &JoinHandle<()>;
}

pub(crate) struct StateCommitter {
    task_performer_handle: JoinHandle<()>,
}

impl StateCommitterTrait for StateCommitter {
    fn create(
        tasks_receiver: Receiver<CommitmentTaskInput>,
        results_sender: Sender<CommitmentTaskOutput>,
        committer_client: SharedCommitterClient,
    ) -> Self {
        let handle = tokio::spawn(async move {
            Self::perform_commitment_tasks(tasks_receiver, results_sender, committer_client).await;
        });
        Self { task_performer_handle: handle }
    }
    fn get_handle(&self) -> &JoinHandle<()> {
        &self.task_performer_handle
    }
}

impl StateCommitter {
    pub(crate) async fn perform_commitment_tasks(
        mut tasks_receiver: Receiver<CommitmentTaskInput>,
        mut results_sender: Sender<CommitmentTaskOutput>,
        committer_client: SharedCommitterClient,
    ) {
        // Placeholder: simply drain the receiver and do nothing.
        // TODO(Amos): Implement the actual commitment tasks logic.
        while let Some(_task) = tasks_receiver.recv().await {}
    }
}
