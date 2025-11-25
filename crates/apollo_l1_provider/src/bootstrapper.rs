use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use apollo_l1_provider_types::SharedL1ProviderClient;
use apollo_state_sync_types::communication::SharedStateSyncClient;
use indexmap::IndexSet;
use starknet_api::block::BlockNumber;
use starknet_api::transaction::TransactionHash;
use tracing::debug;

// When the Provider gets a commit_block that is too high, it starts bootstrapping.
// The commit is rejected by the provider, so it must use sync to catch up to the height of the
// commit, including that height. The sync task continues until reaching the target height,
// inclusive, and only after the commit_block (from sync) causes the Provider's current height to be
// one above the target height, is the backlog applied. Once done with the sync+backlog, the current
// height should be one above the last commit in the backlog, which makes it ready for the next
// commit_block from the batcher.

/// Caches commits to be applied later. This flow is only relevant while the node is starting up.
#[derive(Clone)]
pub struct Bootstrapper {
    pub catch_up_height: BlockNumber,
    pub sync_retry_interval: Duration,
    pub commit_block_backlog: Vec<CommitBlockBacklog>,
    pub l1_provider_client: SharedL1ProviderClient,
    pub sync_client: SharedStateSyncClient,
    // Keep track of sync task for health checks and logging status.
    pub sync_task_handle: SyncTaskHandle,
    pub n_sync_health_check_failures: Arc<AtomicU8>,
}

impl Bootstrapper {
    // FIXME: this isn't added to configs, since this test shouldn't be made here, it should be
    // handled through a task management layer.
    pub const MAX_HEALTH_CHECK_FAILURES: u8 = 5;

    pub fn new(
        l1_provider_client: SharedL1ProviderClient,
        sync_client: SharedStateSyncClient,
        sync_retry_interval: Duration,
    ) -> Self {
        Self {
            sync_retry_interval,
            commit_block_backlog: Default::default(),
            l1_provider_client,
            sync_client,
            sync_task_handle: SyncTaskHandle::NotStartedYet,
            n_sync_health_check_failures: Default::default(),
            // This is overriden when starting the sync task (e.g., when provider starts
            // bootstrapping).
            catch_up_height: BlockNumber(0),
        }
    }

    /// Check if the caller has caught up with the bootstrapper.
    pub fn is_caught_up(&self, current_provider_height: BlockNumber) -> bool {
        let is_caught_up = current_provider_height > self.catch_up_height;

        self.sync_task_health_check(is_caught_up);

        is_caught_up
    }

    pub fn add_commit_block_to_backlog(
        &mut self,
        committed_txs: IndexSet<TransactionHash>,
        height: BlockNumber,
    ) {
        assert!(
            self.commit_block_backlog
                .last()
                .is_none_or(|commit_block| commit_block.height.unchecked_next() == height),
            "Heights should be sequential."
        );

        debug!("Adding future commit-block to backlog at height: {height}");
        self.commit_block_backlog
            .push(CommitBlockBacklog { height, committed_txs: committed_txs.clone() });
    }

    /// Spawns async task that produces and sends commit block messages to the provider, according
    /// to information from the batcher and sync clients, until the provider is caught up.
    pub fn start_l2_sync(
        &mut self,
        current_provider_height: BlockNumber,
        catch_up_height: BlockNumber,
    ) {
        self.catch_up_height = catch_up_height;
        // FIXME: spawning a task like this is evil.
        // However, we aren't using the task executor, so no choice :(
        // Once we start using a centralized threadpool, spawn through it instead of the
        // tokio runtime.
        let sync_task_handle = tokio::spawn(l2_sync_task(
            self.l1_provider_client.clone(),
            self.sync_client.clone(),
            current_provider_height,
            catch_up_height,
            self.sync_retry_interval,
        ));

        self.sync_task_handle = SyncTaskHandle::Started(sync_task_handle.into());
    }

    pub fn catch_up_height(&self) -> BlockNumber {
        self.catch_up_height
    }

    pub fn sync_started(&self) -> bool {
        matches!(self.sync_task_handle, SyncTaskHandle::Started(_))
    }

    fn sync_task_health_check(&self, is_caught_up: bool) {
        let SyncTaskHandle::Started(sync_task) = &self.sync_task_handle else {
            return;
        };

        if sync_task.is_finished() && !is_caught_up && self.commit_block_backlog.is_empty() {
            let n_failures = 1 + self.n_sync_health_check_failures.fetch_add(1, Ordering::SeqCst);
            if n_failures <= Self::MAX_HEALTH_CHECK_FAILURES {
                debug!(
                    "Sync task complete but not caught up yet, health-check failure number: {} \
                     out of {}",
                    n_failures,
                    Self::MAX_HEALTH_CHECK_FAILURES
                );
            } else {
                panic!("Sync task is stuck, not caught up and no backlog to process.");
            }
        }
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
        debug!(
            "Syncing L1Provider with L2 height: {} to target height: {}",
            current_height, catch_up_height
        );
        let block = sync_client.get_block(current_height).await.inspect_err(|err| debug!("{err}"));

        match block {
            Ok(block) => {
                // No rejected txs in sync blocks.
                let l1_handler_rejected_tx_hashes = Default::default();

                l1_provider_client
                    .commit_block(
                        block.l1_transaction_hashes.into_iter().collect(),
                        l1_handler_rejected_tx_hashes,
                        current_height,
                    )
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
    pub committed_txs: IndexSet<TransactionHash>,
}

#[derive(Clone, Debug, Default)]
pub enum SyncTaskHandle {
    #[default]
    NotStartedYet,
    // Adding `Arc` to make this clonable and since this handle isn't modified.
    Started(Arc<tokio::task::JoinHandle<()>>),
}
