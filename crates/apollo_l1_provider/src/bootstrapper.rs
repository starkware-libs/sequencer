use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use apollo_batcher_types::batcher_types::GetHeightResponse;
use apollo_batcher_types::communication::SharedBatcherClient;
use apollo_l1_provider_types::SharedL1ProviderClient;
use apollo_state_sync_types::communication::SharedStateSyncClient;
use indexmap::IndexSet;
use starknet_api::block::BlockNumber;
use starknet_api::transaction::TransactionHash;
use tokio::sync::OnceCell;
use tracing::{debug, error, info};

pub type LazyCatchUpHeight = Arc<OnceCell<BlockNumber>>;

/// Cache's commits to be applied later. This flow is only relevant while the node is starting up.
#[derive(Clone)]
pub struct Bootstrapper {
    /// The catch-up height for the bootstrapper is the batcher height (unless overridden
    /// explicitly). This value, due to infra constraints as of now, is only fetchable _after_
    /// the provider is running, and not during its initialization, hence we are forced to
    /// lazily fetch it at runtime.
    pub catch_up_height: LazyCatchUpHeight,
    pub sync_retry_interval: Duration,
    pub commit_block_backlog: Vec<CommitBlockBacklog>,
    pub l1_provider_client: SharedL1ProviderClient,
    pub batcher_client: SharedBatcherClient,
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
        batcher_client: SharedBatcherClient,
        sync_client: SharedStateSyncClient,
        sync_retry_interval: Duration,
        catch_up_height: LazyCatchUpHeight,
    ) -> Self {
        Self {
            sync_retry_interval,
            commit_block_backlog: Default::default(),
            l1_provider_client,
            batcher_client,
            sync_client,
            sync_task_handle: SyncTaskHandle::NotStartedYet,
            n_sync_health_check_failures: Default::default(),
            catch_up_height,
        }
    }

    /// Check if the caller has caught up with the bootstrapper.
    /// If catch_up_height is unset, the batcher isn't even ready yet.
    pub fn is_caught_up(&self, current_provider_height: BlockNumber) -> bool {
        let is_caught_up = match self.catch_up_height() {
            Some(catch_up_height) => current_provider_height > catch_up_height,
            None => current_provider_height == BlockNumber(0),
        };

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
    pub async fn start_l2_sync(&mut self, current_provider_height: BlockNumber) {
        // FIXME: spawning a task like this is evil.
        // However, we aren't using the task executor, so no choice :(
        // Once we start using a centralized threadpool, spawn through it instead of the
        // tokio runtime.
        let sync_task_handle = tokio::spawn(l2_sync_task(
            self.l1_provider_client.clone(),
            self.batcher_client.clone(),
            self.sync_client.clone(),
            current_provider_height,
            self.catch_up_height.clone(),
            self.sync_retry_interval,
        ));

        self.sync_task_handle = SyncTaskHandle::Started(sync_task_handle.into());
    }

    pub fn catch_up_height(&self) -> Option<BlockNumber> {
        self.catch_up_height.get().copied()
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

// TODO(noamsp): fix catch up height to use batcher height and not the latest block number in
// storage.
async fn l2_sync_task(
    l1_provider_client: SharedL1ProviderClient,
    batcher_client: SharedBatcherClient,
    sync_client: SharedStateSyncClient,
    mut current_height: BlockNumber,
    catch_up_height: LazyCatchUpHeight,
    retry_interval: Duration,
) {
    info!("Try fetching batcher height to initialize catch up point");
    while !catch_up_height.initialized() {
        let Ok(GetHeightResponse { height: batcher_height }) = batcher_client.get_height().await
        else {
            error!("Batcher height request failed. Retrying...");
            tokio::time::sleep(retry_interval).await;
            continue;
        };

        let Some(batcher_latest_block_number) = batcher_height.prev() else {
            info!("Batcher height is 0, no need to catch up. exiting...");
            return;
        };

        info!("Catch up height set: {batcher_latest_block_number}");
        catch_up_height
            .set(batcher_latest_block_number)
            .expect("This is the only write-point, cannot fail")
    }
    let catch_up_height = *catch_up_height.get().expect("Initialized above");

    while current_height <= catch_up_height {
        // TODO(Gilad): add tracing instrument.
        debug!("Try syncing L1Provider with L2 height: {}", current_height);
        let block = sync_client.get_block(current_height).await.inspect_err(|err| debug!("{err}"));

        match block {
            Ok(Some(block)) => {
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
