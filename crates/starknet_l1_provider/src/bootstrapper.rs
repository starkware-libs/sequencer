use std::sync::Arc;
use std::time::Duration;

use starknet_api::block::BlockNumber;
use starknet_api::transaction::TransactionHash;
use starknet_l1_provider_types::SharedL1ProviderClient;
use starknet_state_sync_types::communication::SharedStateSyncClient;
use tracing::debug;

/// Cache's commits to be applied later. This flow is only relevant while the node is starting up.
#[derive(Clone)]
pub struct Bootstrapper {
    pub catch_up_height: BlockNumber,
    pub sync_retry_interval: Duration,
    pub commit_block_backlog: Vec<CommitBlockBacklog>,
    pub l1_provider_client: SharedL1ProviderClient,
    pub sync_client: SharedStateSyncClient,
    // Keep track of sync task for health checks and logging status.
    pub sync_task_handle: SyncTaskHandle,
}

impl Bootstrapper {
    /// Check if the caller has caught up with the bootstrapper.
    pub fn is_caught_up(&self, current_provider_height: BlockNumber) -> bool {
        self.catch_up_height == current_provider_height
        // TODO(Gilad): add health_check here, making sure that the sync task isn't stuck, which is
        // `handle dropped && backlog empty && not caught up`.
    }

    pub fn add_commit_block_to_backlog(
        &mut self,
        committed_txs: &[TransactionHash],
        height: BlockNumber,
    ) {
        assert!(
            self.commit_block_backlog
                .last()
                .is_none_or(|commit_block| commit_block.height.unchecked_next() == height),
            "Heights should be sequential."
        );

        self.commit_block_backlog
            .push(CommitBlockBacklog { height, committed_txs: committed_txs.to_vec() });
    }

    /// Spawns async task that produces and sends commit block messages to the provider, according
    /// to information from the sync client, until the provider is caught up.
    pub async fn start_l2_sync(&mut self, current_provider_height: BlockNumber) {
        // FIXME: spawning a task like this is evil.
        // However, we aren't using the task executor, so no choice :(
        // Once we start using a centralized threadpool, spawn through it instead of the
        // tokio runtime.
        let sync_task_handle = tokio::spawn(l2_sync_task(
            self.l1_provider_client.clone(),
            self.sync_client.clone(),
            current_provider_height,
            self.catch_up_height,
            self.sync_retry_interval,
        ));

        self.sync_task_handle = SyncTaskHandle::Started(sync_task_handle.into());
    }
}

impl PartialEq for Bootstrapper {
    fn eq(&self, other: &Self) -> bool {
        self.catch_up_height == other.catch_up_height
            && self.commit_block_backlog == other.commit_block_backlog
    }
}

impl Eq for Bootstrapper {}

impl std::fmt::Debug for Bootstrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bootstrapper")
            .field("catch_up_height", &self.catch_up_height)
            .field("commit_block_backlog", &self.commit_block_backlog)
            .field("sync_task_handle", &self.sync_task_handle)
            .finish_non_exhaustive()
    }
}

async fn l2_sync_task(
    l1_provider_client: SharedL1ProviderClient,
    sync_client: SharedStateSyncClient,
    mut current_height: BlockNumber,
    catch_up_height: BlockNumber,
    retry_interval: Duration,
) {
    while current_height <= catch_up_height {
        // TODO(Gilad): add tracing instrument.
        debug!("Try syncing L1Provider with L2 height: {}", current_height);
        let block = sync_client.get_block(current_height).await.inspect_err(|err| debug!("{err}"));

        match block {
            Ok(Some(block)) => {
                // FIXME: block.transaction_hashes should be `block.l1_transaction_hashes`!
                l1_provider_client
                    .commit_block(block.transaction_hashes, current_height)
                    .await
                    .unwrap();
                current_height = current_height.unchecked_next();
            }
            _ => tokio::time::sleep(retry_interval).await,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommitBlockBacklog {
    pub height: BlockNumber,
    pub committed_txs: Vec<TransactionHash>,
}

#[derive(Clone, Debug, Default)]
pub enum SyncTaskHandle {
    #[default]
    NotStartedYet,
    // Adding `Arc` to make this clonable and since this handle isn't modified.
    Started(Arc<tokio::task::JoinHandle<()>>),
}
