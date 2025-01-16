use std::sync::Arc;
use std::time::Duration;

use starknet_api::block::BlockNumber;
use starknet_api::transaction::TransactionHash;
use starknet_l1_provider_types::SharedL1ProviderClient;
use starknet_state_sync_types::communication::SharedStateSyncClient;
use tokio::time::sleep;
use tracing::{debug, error};

#[derive(Clone)]
pub struct Bootstrapper {
    pub catch_up_height: BlockNumber,
    pub commit_block_backlog: Vec<CommitBlockBacklog>,
}

impl Bootstrapper {
    pub fn is_caught_up(&self, current_provider_height: BlockNumber) -> bool {
        let is_complete = self.catch_up_height == current_provider_height;
        self.health_check(is_complete);
        is_complete
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

    pub fn start_l2_sync(&mut self) {
        let sync_task_handle = tokio::spawn(l2_sync_task(
            self.l1_provider_client.clone(),
            self.sync_client.clone(),
            self.current_provider_height,
            self.catch_up_height,
        ));

        self.sync_task_handle = SyncTaskHandle::Started(sync_task_handle.into());
    }

    /// Sanity check: checks if the bootstrapper is stuck and cannot reach the catch up height.
    /// This check is equivalent to checking if the sync task was prematurely dropped.
    fn health_check(&self, is_complete: bool) {
        if !is_complete && self.sync_task_handle.is_finished() {
            error!(
                "Bootstrapper is stuck, and L1 Provider will never catch up to the batcher height"
            );
        }
    }
}

impl PartialEq for Bootstrapper {
    fn eq(&self, other: &Self) -> bool {
        self.catch_up_height == other.catch_up_height
            && self.current_provider_height == other.current_provider_height
            && self.commit_block_backlog == other.commit_block_backlog
    }
}

impl Eq for Bootstrapper {}

impl std::fmt::Debug for Bootstrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bootstrapper")
            .field("catch_up_height", &self.catch_up_height)
            .field("current_provider_height", &self.current_provider_height)
            .field("commit_block_backlog", &self.commit_block_backlog)
            .field("l1_provider_client", &"<non-debuggable>")
            .field("sync_client", &"<non-debuggable>")
            .field("sync_task_handle", &self.sync_task_handle)
            .finish()
    }
}

async fn l2_sync_task(
    l1_provider_client: SharedL1ProviderClient,
    sync_client: SharedStateSyncClient,
    mut current_height: BlockNumber,
    catch_up_height: BlockNumber,
) {
    while current_height < catch_up_height {
        // TODO: add tracing instrument.
        debug!("Try syncing L1Provider with L2: {}", current_height);
        let next_height = current_height.unchecked_next();
        let block = sync_client.get_block(next_height).await.inspect_err(|err| debug!("{err}"));

        match block {
            Ok(Some(block)) => {
                // FIXME: block.transaction_hashes should be `block.l1_transaction_hashes`!
                l1_provider_client
                    .commit_block(block.transaction_hashes, current_height)
                    .await
                    .unwrap();
                current_height = current_height.unchecked_next();
            }
            // TODO: is this a good timeout value?
            // TODO: make configurable.
            _ => sleep(Duration::from_secs(1)).await,
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
    // Adding Arc to make this clonable and since this handle isn't modified.
    Started(Arc<tokio::task::JoinHandle<()>>),
}

impl SyncTaskHandle {
    fn is_finished(&self) -> bool {
        match self {
            SyncTaskHandle::NotStartedYet => false,
            SyncTaskHandle::Started(handle) => handle.is_finished(),
        }
    }
}
