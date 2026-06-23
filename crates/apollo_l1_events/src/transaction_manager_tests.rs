use std::collections::BTreeMap;

use indexmap::IndexMap;
use starknet_api::block::{BlockTimestamp, UnixTimestamp};
use starknet_api::transaction::TransactionHash;

use super::{StagingEpoch, TransactionManager, TransactionManagerConfig};
use crate::test_utils::l1_handler;
use crate::transaction_record::{TransactionPayload, TransactionRecord};

/// Builds a `Pending`, full-payload record. The L1 block timestamp (what the metric reports) is
/// set independently of the scrape timestamp so tests can distinguish the two.
fn pending_record(
    tx_hash: usize,
    l1_block_timestamp: u64,
    scrape_timestamp: UnixTimestamp,
) -> (TransactionHash, UnixTimestamp, TransactionRecord) {
    let tx = l1_handler(tx_hash);
    let hash = tx.tx_hash;
    let record = TransactionRecord::new(TransactionPayload::Full {
        tx,
        created_at_block_timestamp: BlockTimestamp(l1_block_timestamp),
        scrape_timestamp,
    });
    (hash, scrape_timestamp, record)
}

fn manager_with_pending(
    pending: Vec<(TransactionHash, UnixTimestamp, TransactionRecord)>,
) -> TransactionManager {
    let mut records = IndexMap::new();
    let mut proposable_index: BTreeMap<UnixTimestamp, Vec<TransactionHash>> = BTreeMap::new();
    for (hash, scrape_timestamp, record) in pending {
        proposable_index.entry(scrape_timestamp).or_default().push(hash);
        records.insert(hash, record);
    }
    TransactionManager::create_for_testing(
        records.into(),
        proposable_index,
        StagingEpoch::new(),
        TransactionManagerConfig::default(),
        BTreeMap::new(),
    )
}

#[test]
fn oldest_pending_l1_block_timestamp_returns_min_block_timestamp() {
    // Scrape order (proposable index key) deliberately differs from L1 block order: the tx
    // scraped first (key 10) has the newest L1 timestamp, so a "first entry" implementation
    // would report the wrong value.
    let manager = manager_with_pending(vec![
        pending_record(1, 300, 10),
        pending_record(2, 100, 20),
        pending_record(3, 200, 30),
    ]);
    assert_eq!(manager.oldest_pending_l1_block_timestamp(), Some(BlockTimestamp(100)));
}

#[test]
fn oldest_pending_l1_block_timestamp_is_none_when_nothing_pending() {
    assert_eq!(manager_with_pending(vec![]).oldest_pending_l1_block_timestamp(), None);
}
