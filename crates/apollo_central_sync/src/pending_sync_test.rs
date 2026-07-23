use std::sync::Arc;
use std::time::Duration;

use apollo_starknet_client::reader::PendingData;
use papyrus_common::pending_classes::PendingClasses;
use starknet_api::block::BlockHash;
use starknet_api::felt;
use tokio::sync::RwLock;

use crate::pending_sync::sync_pending_data;
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
/// The anchor must come from the caller's tip (central tip / channel marker); the function must
/// honor the passed-in hash rather than re-deriving the tip from a read-only storage snapshot,
/// which batched writes can leave stale.
#[tokio::test]
async fn collects_pending_data_anchored_on_given_hash() {
    let tip_block_hash = BlockHash(felt!("0x111"));
    let next_block_hash = BlockHash(felt!("0x222"));

    // First poll returns the pending block sitting on top of the real tip; the second poll reports
    // a new block (different parent), which ends pending sync.
    let mut seq = mockall::Sequence::new();
    let mut pending_source = MockPendingSourceTrait::new();
    for parent_block_hash in [tip_block_hash, next_block_hash] {
        pending_source
            .expect_get_pending_data()
            .times(1)
            .returning(move || Ok(pending_data_with_parent(parent_block_hash)))
            .in_sequence(&mut seq);
    }

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

/// If the anchor is stale (a tip behind the real one — e.g. a batched write the read-only snapshot
/// hasn't caught up to), `sync_pending_data` immediately concludes a new block appeared and
/// collects nothing — demonstrating why the caller must pass an accurate tip rather than a lagging
/// read-only-snapshot read.
#[tokio::test]
async fn stale_anchor_collects_nothing() {
    let real_tip_block_hash = BlockHash(felt!("0x111"));
    let stale_block_hash = BlockHash(felt!("0x999"));

    let mut pending_source = MockPendingSourceTrait::new();
    pending_source
        .expect_get_pending_data()
        .times(1)
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
