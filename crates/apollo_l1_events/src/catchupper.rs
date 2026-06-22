use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use apollo_l1_events_types::errors::L1EventsProviderError;
use apollo_l1_events_types::{L1EventsProviderResult, SharedL1EventsProviderClient};
use apollo_state_sync_types::communication::SharedStateSyncClient;
use indexmap::IndexSet;
use starknet_api::block::BlockNumber;
use starknet_api::transaction::TransactionHash;
use tracing::{debug, warn};

use crate::metrics::L1_MESSAGE_PROVIDER_COMMIT_BLOCK_BACKLOG_LEN;

// When the Provider gets a commit_block that is too high, it starts catching up.
// The commit is rejected by the provider, so it must use sync to catch up to the height of the
// commit, including that height. The sync task continues until reaching the target height,
// inclusive, and only after the commit_block (from sync) causes the Provider's current height to be
// one above the target height, is the backlog applied. Once done with the sync+backlog, the current
// height should be one above the last commit in the backlog, which makes it ready for the next
// commit_block from the batcher.

/// Caches commits to be applied later. This flow is only relevant while the node is starting up.
#[derive(Clone)]
pub struct Catchupper {
    // Shared with the running sync task (rather than passed by value) so the target can be raised
    // while the task is in flight; see `update_target_height`.
    pub target_height: Arc<AtomicU64>,
    pub sync_retry_interval: Duration,
    pub commit_block_backlog: Vec<CommitBlockBacklog>,
    pub l1_events_provider_client: SharedL1EventsProviderClient,
    pub sync_client: SharedStateSyncClient,
    // Keep track of sync task for health checks and logging status.
    pub sync_task_handle: SyncTaskHandle,
    pub n_sync_health_check_failures: Arc<AtomicU8>,
    /// Cap on `commit_block_backlog` length. Exceeding it is a hard error, not a drop, to preserve
    /// the gapless-sequential invariant of the backlog.
    pub max_commit_block_backlog_len: usize,
}

impl Catchupper {
    // FIXME: this isn't added to configs, since this test shouldn't be made here, it should be
    // handled through a task management layer.
    pub const MAX_HEALTH_CHECK_FAILURES: u8 = 5;

    pub fn new(
        l1_events_provider_client: SharedL1EventsProviderClient,
        sync_client: SharedStateSyncClient,
        sync_retry_interval: Duration,
        max_commit_block_backlog_len: usize,
    ) -> Self {
        Self {
            sync_retry_interval,
            commit_block_backlog: Default::default(),
            l1_events_provider_client,
            sync_client,
            sync_task_handle: SyncTaskHandle::NotStartedYet,
            n_sync_health_check_failures: Default::default(),
            max_commit_block_backlog_len,
            // This is overriden when starting the sync task (e.g., when provider starts
            // catching up).
            target_height: Default::default(),
        }
    }

    /// Check if the caller has caught up with the catchupper.
    pub fn is_caught_up(&self, current_provider_height: BlockNumber) -> bool {
        let is_caught_up = current_provider_height > self.target_height();

        self.sync_task_health_check(is_caught_up);

        is_caught_up
    }

    pub fn add_commit_block_to_backlog(
        &mut self,
        committed_txs: IndexSet<TransactionHash>,
        height: BlockNumber,
    ) -> L1EventsProviderResult<()> {
        assert!(
            self.commit_block_backlog
                .last()
                .is_none_or(|commit_block| commit_block.height.unchecked_next() == height),
            "Heights should be sequential."
        );

        // Bound growth on a stalled/lagging L2 sync. We must NOT drop entries: the backlog is a
        // gapless, strictly-sequential run and a hole would corrupt the drain-time sequentiality
        // assert and silently skip an L1-handler commit. Fail loudly instead.
        if self.commit_block_backlog.len() >= self.max_commit_block_backlog_len {
            warn!(
                "Catch-up commit-block backlog reached its cap of {} entries at height {height}; \
                 rejecting commit-block. L2 sync is likely stalled or lagging.",
                self.max_commit_block_backlog_len
            );
            return Err(L1EventsProviderError::CatchUpBacklogOverflow {
                height,
                max: self.max_commit_block_backlog_len,
            });
        }

        debug!("Adding future commit-block to backlog at height: {height}");
        self.commit_block_backlog
            .push(CommitBlockBacklog { height, committed_txs: committed_txs.clone() });
        L1_MESSAGE_PROVIDER_COMMIT_BLOCK_BACKLOG_LEN.set_lossy(self.commit_block_backlog.len());
        Ok(())
    }

    /// Spawns async task that produces and sends commit block messages to the provider, according
    /// to information from the batcher and sync clients, until the provider is caught up.
    pub fn start_l2_sync(
        &mut self,
        current_provider_height: BlockNumber,
        target_height: BlockNumber,
    ) {
        // Fresh shared target for this task; cloned into the task below so it shares the same cell.
        self.target_height = Arc::new(AtomicU64::new(target_height.0));
        // FIXME: spawning a task like this is evil.
        // However, we aren't using the task executor, so no choice :(
        // Once we start using a centralized threadpool, spawn through it instead of the
        // tokio runtime.
        let sync_task_handle = tokio::spawn(l2_sync_task(
            self.l1_events_provider_client.clone(),
            self.sync_client.clone(),
            current_provider_height,
            self.target_height.clone(),
            self.sync_retry_interval,
        ));

        self.sync_task_handle = SyncTaskHandle::Started(sync_task_handle.into());
    }

    pub fn target_height(&self) -> BlockNumber {
        BlockNumber(self.target_height.load(Ordering::Acquire))
    }

    /// Raises the target height of the running sync task so it keeps syncing up to `target_height`.
    /// Uses `fetch_max` so the target only moves forward; a lower height is ignored.
    pub fn update_target_height(&self, target_height: BlockNumber) {
        self.target_height.fetch_max(target_height.0, Ordering::Release);
    }

    /// Returns true while an L2 sync task is in flight (spawned and not yet finished).
    pub fn is_sync_task_running(&self) -> bool {
        matches!(&self.sync_task_handle, SyncTaskHandle::Started(sync_task) if !sync_task.is_finished())
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

impl PartialEq for Catchupper {
    fn eq(&self, other: &Self) -> bool {
        self.target_height() == other.target_height()
            && self.commit_block_backlog == other.commit_block_backlog
    }
}

impl Eq for Catchupper {}

impl std::fmt::Debug for Catchupper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Catchupper")
            .field("target_height", &self.target_height())
            .field("commit_block_backlog", &self.commit_block_backlog)
            .field("sync_task_handle", &self.sync_task_handle)
            .finish_non_exhaustive()
    }
}

async fn l2_sync_task(
    l1_events_provider_client: SharedL1EventsProviderClient,
    sync_client: SharedStateSyncClient,
    mut current_height: BlockNumber,
    target_height: Arc<AtomicU64>,
    retry_interval: Duration,
) {
    // The target is re-read every iteration so an `update_target_height` call from the provider
    // (a higher block committed before catch-up finishes) extends this same task instead of
    // spawning a competing one.
    while current_height.0 <= target_height.load(Ordering::Acquire) {
        // TODO(Gilad): add tracing instrument.
        debug!(
            "Syncing L1EventsProvider with L2 height: {} to target height: {}",
            current_height,
            target_height.load(Ordering::Acquire)
        );
        let block = sync_client.get_block(current_height).await.inspect_err(|err| debug!("{err}"));

        match block {
            Ok(block) => {
                // No rejected txs in sync blocks.
                let l1_handler_rejected_tx_hashes = Default::default();

                let result = l1_events_provider_client
                    .commit_block(
                        block.l1_transaction_hashes.into_iter().collect(),
                        l1_handler_rejected_tx_hashes,
                        current_height,
                    )
                    .await;
                if let Err(err) = result {
                    warn!(?err, "Failed to commit block to L1 events provider.");
                    tokio::time::sleep(retry_interval).await;
                    continue; // Retry the sync task.
                }
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
