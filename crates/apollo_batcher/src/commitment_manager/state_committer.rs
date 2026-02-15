use std::time::Duration;

use apollo_committer_types::committer_types::{CommitBlockRequest, RevertBlockRequest};
use apollo_committer_types::communication::SharedCommitterClient;
use apollo_committer_types::errors::CommitterClientResult;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task::JoinHandle;
use tracing::{error, info};

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

    #[allow(dead_code)]
    /// Returns a handle to the thread performing commitment tasks.
    fn get_handle(&self) -> &JoinHandle<()>;
}

pub(crate) struct StateCommitter {
    #[allow(dead_code)]
    task_performer_handle: JoinHandle<()>,
}

impl StateCommitterTrait for StateCommitter {
    fn create(
        tasks_receiver: Receiver<CommitterTaskInput>,
        results_sender: Sender<CommitterTaskOutput>,
        committer_client: SharedCommitterClient,
    ) -> Self {
        let handle = tokio::spawn(async move {
            Self::perform_tasks(tasks_receiver, results_sender, committer_client).await;
        });
        Self { task_performer_handle: handle }
    }
    fn get_handle(&self) -> &JoinHandle<()> {
        &self.task_performer_handle
    }
}

impl StateCommitter {
    /// Performs the tasks in the channel. Retries at recoverable errors.
    pub(crate) async fn perform_tasks(
        mut tasks_receiver: Receiver<CommitterTaskInput>,
        results_sender: Sender<CommitterTaskOutput>,
        committer_client: SharedCommitterClient,
    ) {
        // TODO(Yoav): Test this function.
        info!("StateCommitter task loop running.");
        while let Some(request) = tasks_receiver.recv().await {
            info!(
                "Processing task of type {:?} for height {:?}",
                request.task_type(),
                request.height(),
            );
            let output = perform_task(request, &committer_client).await;
            let height = output.height();
            match results_sender.send(output.clone()).await {
                Ok(_) => {
                    info!(
                        "Successfully sent the committer result to the results channel: \
                         {output:?}."
                    );
                }
                Err(err) => panic!("Failed to send results for height {height}. error: {err}"),
            }
        }
    }
}

/// Performs a commitment task by calling the committer.
/// Retries at errors.
async fn perform_task(
    request: CommitterTaskInput,
    committer_client: &SharedCommitterClient,
) -> CommitterTaskOutput {
    loop {
        let result = match &request {
            CommitterTaskInput::Commit(commit_block_request) => {
                perform_commit_block_task(commit_block_request.clone(), committer_client).await
            }
            CommitterTaskInput::Revert(revert_block_request) => {
                perform_revert_block_task(revert_block_request.clone(), committer_client).await
            }
        };
        match result {
            Ok(output) => return output,
            Err(err) => {
                error!("Committer error: {err:?}. retrying...");
            }
        };
        tokio::time::sleep(Duration::from_millis(500)).await;
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
