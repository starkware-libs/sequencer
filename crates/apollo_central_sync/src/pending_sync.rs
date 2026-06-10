use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use apollo_starknet_client::reader::{DeclaredClassHashEntry, PendingData};
use futures::stream::FuturesUnordered;
use futures_util::{FutureExt, StreamExt};
use papyrus_common::pending_classes::{PendingClasses, PendingClassesTrait};
use starknet_api::block::BlockHash;
use starknet_api::core::ClassHash;
use tokio::sync::RwLock;
use tracing::{debug, trace};

use crate::sources::central::CentralSourceTrait;
use crate::sources::pending::PendingSourceTrait;
use crate::StateSyncError;

// Update the pending data and return when a new block is discovered.
pub(crate) async fn sync_pending_data<
    TPendingSource: PendingSourceTrait + Sync + Send + 'static,
    TCentralSource: CentralSourceTrait + Sync + Send + 'static,
>(
    // The hash of the latest synced block, used to anchor the pending block. The caller must pass
    // the tip it used to decide we are caught up (the central tip / channel marker), NOT a value
    // re-read from a read-only storage snapshot: with batched writes the committed-to-disk view
    // can lag the channel marker, which would anchor pending data on a stale block and starve the
    // pending feed.
    latest_block_hash: BlockHash,
    central_source: Arc<TCentralSource>,
    pending_source: Arc<TPendingSource>,
    pending_data: Arc<RwLock<PendingData>>,
    pending_classes: Arc<RwLock<PendingClasses>>,
    sleep_duration: Duration,
) -> Result<(), StateSyncError> {
    let mut tasks = FuturesUnordered::new();
    tasks.push(
        get_pending_data(
            latest_block_hash,
            pending_source.clone(),
            pending_data.clone(),
            pending_classes.clone(),
            Duration::ZERO,
        )
        .boxed(),
    );
    let mut processed_classes = HashSet::new();
    let mut processed_compiled_classes = HashSet::new();
    loop {
        match tasks.next().await.expect("There should always be a task in the pending sync")? {
            PendingSyncTaskResult::PendingSyncFinished => return Ok(()),
            PendingSyncTaskResult::DownloadedNewPendingData => {
                let (declared_classes, old_declared_contracts) = {
                    // TODO(shahak): Consider getting the pending data from the task result instead
                    // of reading from the lock.
                    let pending_state_diff = &pending_data.read().await.state_update.state_diff;
                    (
                        pending_state_diff.declared_classes.clone(),
                        pending_state_diff.old_declared_contracts.clone(),
                    )
                };
                for DeclaredClassHashEntry { class_hash, compiled_class_hash } in declared_classes {
                    if processed_classes.insert(class_hash) {
                        tasks.push(
                            get_pending_class(
                                class_hash,
                                central_source.clone(),
                                pending_classes.clone(),
                            )
                            .boxed(),
                        );
                    }
                    if processed_compiled_classes.insert(compiled_class_hash) {
                        tasks.push(
                            get_pending_compiled_class(
                                class_hash,
                                central_source.clone(),
                                pending_classes.clone(),
                            )
                            .boxed(),
                        );
                    }
                }
                for class_hash in old_declared_contracts {
                    if processed_classes.insert(class_hash) {
                        tasks.push(
                            get_pending_class(
                                class_hash,
                                central_source.clone(),
                                pending_classes.clone(),
                            )
                            .boxed(),
                        );
                    }
                }
                tasks.push(
                    get_pending_data(
                        latest_block_hash,
                        pending_source.clone(),
                        pending_data.clone(),
                        pending_classes.clone(),
                        sleep_duration,
                    )
                    .boxed(),
                )
            }
            PendingSyncTaskResult::DownloadedOldPendingData => tasks.push(
                get_pending_data(
                    latest_block_hash,
                    pending_source.clone(),
                    pending_data.clone(),
                    pending_classes.clone(),
                    sleep_duration,
                )
                .boxed(),
            ),
            PendingSyncTaskResult::DownloadedClassOrCompiledClass => {}
        }
    }
}

enum PendingSyncTaskResult {
    DownloadedNewPendingData,
    DownloadedOldPendingData,
    PendingSyncFinished,
    DownloadedClassOrCompiledClass,
}

async fn get_pending_data<TPendingSource: PendingSourceTrait + Sync + Send + 'static>(
    latest_block_hash: BlockHash,
    pending_source: Arc<TPendingSource>,
    pending_data: Arc<RwLock<PendingData>>,
    pending_classes: Arc<RwLock<PendingClasses>>,
    sleep_duration: Duration,
) -> Result<PendingSyncTaskResult, StateSyncError> {
    tokio::time::sleep(sleep_duration).await;

    let new_pending_data = pending_source.get_pending_data().await?;

    // In Starknet, if there's no pending block then the latest block is returned. We prefer to
    // treat this case as if the pending block is an empty block on top of the latest block.
    // We distinguish this case by looking if the block_hash field is present.
    let new_pending_parent_hash =
        new_pending_data.block.block_hash().unwrap_or(new_pending_data.block.parent_block_hash());
    if new_pending_parent_hash != latest_block_hash {
        // TODO(shahak): If block_hash is present, consider writing the pending data here so that
        // the pending data will be available until the node syncs on the new block.
        debug!("A new block was found. Stopping pending sync.");
        return Ok(PendingSyncTaskResult::PendingSyncFinished);
    };

    let (current_pending_num_transactions, current_pending_parent_hash) = {
        let pending_block = &pending_data.read().await.block;
        (
            pending_block.transactions().len(),
            pending_block.block_hash().unwrap_or(pending_block.parent_block_hash()),
        )
    };
    let is_new_pending_data_more_advanced = current_pending_parent_hash != new_pending_parent_hash
        || new_pending_data.block.transactions().len() > current_pending_num_transactions;
    if is_new_pending_data_more_advanced {
        debug!("Received new pending data.");
        trace!("Pending data: {new_pending_data:#?}.");
        if current_pending_parent_hash != new_pending_parent_hash {
            pending_classes.write().await.clear();
        }
        *pending_data.write().await = new_pending_data;
        Ok(PendingSyncTaskResult::DownloadedNewPendingData)
    } else {
        debug!("Pending block wasn't updated. Waiting for pending block to be updated.");
        Ok(PendingSyncTaskResult::DownloadedOldPendingData)
    }
}

async fn get_pending_class<TCentralSource: CentralSourceTrait + Sync + Send + 'static>(
    class_hash: ClassHash,
    central_source: Arc<TCentralSource>,
    pending_classes: Arc<RwLock<PendingClasses>>,
) -> Result<PendingSyncTaskResult, StateSyncError> {
    let class = central_source.get_class(class_hash).await?;
    pending_classes.write().await.add_class(class_hash, class);
    Ok(PendingSyncTaskResult::DownloadedClassOrCompiledClass)
}

async fn get_pending_compiled_class<TCentralSource: CentralSourceTrait + Sync + Send + 'static>(
    class_hash: ClassHash,
    central_source: Arc<TCentralSource>,
    pending_classes: Arc<RwLock<PendingClasses>>,
) -> Result<PendingSyncTaskResult, StateSyncError> {
    let compiled_class = central_source.get_compiled_class(class_hash).await?;
    pending_classes.write().await.add_compiled_class(class_hash, compiled_class);
    Ok(PendingSyncTaskResult::DownloadedClassOrCompiledClass)
}

#[cfg(test)]
mod pending_sync_test {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use apollo_starknet_client::reader::PendingData;
    use papyrus_common::pending_classes::PendingClasses;
    use starknet_api::block::BlockHash;
    use starknet_api::felt;
    use tokio::sync::RwLock;

    use super::sync_pending_data;
    use crate::sources::central::MockCentralSourceTrait;
    use crate::sources::pending::MockPendingSourceTrait;

    fn pending_data_with_parent(parent_block_hash: BlockHash) -> PendingData {
        let mut pending_data = PendingData::default();
        *pending_data.block.parent_block_hash_mutable() = parent_block_hash;
        pending_data
    }

    /// `sync_pending_data` must anchor on the `latest_block_hash` it is given: pending data whose
    /// parent matches that hash is collected.
    ///
    /// Regression for L917: the anchor must come from the caller's tip (central tip / channel
    /// marker), so this function must honor the passed-in hash rather than re-deriving the tip
    /// from a RO storage snapshot (which batched writes can leave stale).
    #[tokio::test]
    async fn collects_pending_data_anchored_on_given_hash() {
        let tip_block_hash = BlockHash(felt!("0x111"));
        let next_block_hash = BlockHash(felt!("0x222"));

        // First poll returns the pending block sitting on top of the real tip; the second poll
        // reports a new block (different parent), which ends pending sync.
        let n_calls = Arc::new(AtomicUsize::new(0));
        let mut pending_source = MockPendingSourceTrait::new();
        pending_source.expect_get_pending_data().returning(move || {
            let parent_block_hash = if n_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                tip_block_hash
            } else {
                next_block_hash
            };
            Ok(pending_data_with_parent(parent_block_hash))
        });

        let pending_data = Arc::new(RwLock::new(PendingData::default()));
        sync_pending_data(
            tip_block_hash,
            Arc::new(MockCentralSourceTrait::new()),
            Arc::new(pending_source),
            pending_data.clone(),
            Arc::new(RwLock::new(PendingClasses::default())),
            Duration::ZERO,
        )
        .await
        .expect("sync_pending_data should return Ok when a new block ends the poll");

        assert_eq!(
            pending_data.read().await.block.parent_block_hash(),
            tip_block_hash,
            "pending data for the block on top of the given tip should have been collected"
        );
    }

    /// If the anchor is stale (the L917 skew: a tip behind the real one), `sync_pending_data`
    /// immediately concludes a new block appeared and collects nothing — demonstrating why the
    /// caller must pass an accurate tip rather than a lagging RO-snapshot read.
    #[tokio::test]
    async fn stale_anchor_collects_nothing() {
        let real_tip_block_hash = BlockHash(felt!("0x111"));
        let stale_block_hash = BlockHash(felt!("0x999"));

        let mut pending_source = MockPendingSourceTrait::new();
        pending_source
            .expect_get_pending_data()
            .returning(move || Ok(pending_data_with_parent(real_tip_block_hash)));

        let pending_data = Arc::new(RwLock::new(PendingData::default()));
        sync_pending_data(
            stale_block_hash,
            Arc::new(MockCentralSourceTrait::new()),
            Arc::new(pending_source),
            pending_data.clone(),
            Arc::new(RwLock::new(PendingClasses::default())),
            Duration::ZERO,
        )
        .await
        .expect("sync_pending_data should return Ok");

        assert_ne!(
            pending_data.read().await.block.parent_block_hash(),
            real_tip_block_hash,
            "with a stale anchor, the real tip's pending data must not be collected"
        );
    }
}
