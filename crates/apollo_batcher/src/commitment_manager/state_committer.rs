#![allow(dead_code, unused_variables, unused_mut)]

use apollo_committer_types::committer_types::{CommitBlockRequest, RevertBlockRequest};
use apollo_committer_types::communication::{CommitterRequest, SharedCommitterClient};
use apollo_committer_types::errors::{CommitterClientError, CommitterClientResult};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;

use crate::commitment_manager::types::{
    CommitmentTaskOutput,
    CommitterTaskInput,
    CommitterTaskOutput,
    RevertTaskOutput,
};

/// Commits state changes by calling the committer.
pub(crate) trait StateCommitterTrait {
    /// Creates a new instance and starts thread which performs commitment tasks.
    fn create(
        tasks_receiver: Receiver<CommitterTaskInput>,
        results_sender: Sender<CommitterTaskOutput>,
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
        tasks_receiver: Receiver<CommitterTaskInput>,
        results_sender: Sender<CommitterTaskOutput>,
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
    /// Repeatedly performs any task in the channel.
    pub(crate) async fn perform_commitment_tasks(
        mut tasks_receiver: Receiver<CommitterTaskInput>,
        mut results_sender: Sender<CommitterTaskOutput>,
        committer_client: SharedCommitterClient,
    ) {
        while let Some(CommitterTaskInput(request)) = tasks_receiver.recv().await {
            let output = perform_commitment_task(request, &committer_client).await;
            // TODO(Yoav): wait for task channel by config.
            results_sender.send(output).await.unwrap();
        }
    }
}

/// Performs a commitment task by calling the committer.
/// Retries at recoverable errors.
async fn perform_commitment_task(
    request: CommitterRequest,
    committer_client: &SharedCommitterClient,
) -> CommitterTaskOutput {
    loop {
        let result = match &request {
            CommitterRequest::CommitBlock(commit_block_request) => {
                perform_commit_block_task(commit_block_request.clone(), committer_client).await
            }
            CommitterRequest::RevertBlock(revert_block_request) => {
                perform_revert_block_task(revert_block_request.clone(), committer_client).await
            }
        };
        match result {
            Ok(output) => return output,
            Err(err) => {
                handle_task_error(err).await;
                continue;
            }
        }
    }
}

async fn perform_commit_block_task(
    commit_block_request: CommitBlockRequest,
    committer_client: &SharedCommitterClient,
) -> CommitterClientResult<CommitterTaskOutput> {
    let height = commit_block_request.height;
    let response = committer_client.commit_block(commit_block_request).await?;
    Ok(CommitterTaskOutput::Commit(CommitmentTaskOutput { response, height }))
}

async fn perform_revert_block_task(
    revert_block_request: RevertBlockRequest,
    committer_client: &SharedCommitterClient,
) -> CommitterClientResult<CommitterTaskOutput> {
    let height = revert_block_request.height;
    let response = committer_client.revert_block(revert_block_request).await?;
    Ok(CommitterTaskOutput::Revert(RevertTaskOutput { response, height }))
}

/// Panics on unrecoverable errors.
async fn handle_task_error(error: CommitterClientError) {
    // TODO(Amos): Handle errors.
    unimplemented!();
}
