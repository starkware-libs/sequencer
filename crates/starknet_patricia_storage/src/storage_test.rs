use std::path::Path;

use rstest::rstest;
use tokio::task::JoinSet;

use crate::mdbx_storage::MdbxStorage;
use crate::rocksdb_storage::{RocksDbOptions, RocksDbStorage};
use crate::storage_trait::{AsyncStorage, DbKey, DbValue};

/// Tests the concurrent access to the storage. Explicitly uses 11 worker threads to get actual
/// parallelism (one thread for main test, 10 worker threads for concurrent operations).
#[rstest]
#[case::rocksdb_storage(
    RocksDbStorage::open(
        Path::new("/tmp/test_rocksdb_storage"), RocksDbOptions::default(), false
    ).unwrap()
)]
#[case::mdbx_storage(MdbxStorage::open(Path::new("/tmp/test_mdbx_storage")).unwrap())]
#[tokio::test(flavor = "multi_thread", worker_threads = 11)]
async fn test_storage_concurrent_access(#[case] mut storage: impl AsyncStorage) {
    // Parallel writes to the storage.
    let mut tasks = JoinSet::new();

    for i in 0..10u8 {
        let mut cloned_storage = storage.clone();
        tasks.spawn(async move {
            cloned_storage.set(DbKey(vec![i]), DbValue(vec![i])).unwrap();
        });
    }

    tasks.join_all().await;

    for i in 0..10u8 {
        assert_eq!(storage.get(&DbKey(vec![i])).unwrap(), Some(DbValue(vec![i])));
    }

    // Parallel reads from the storage while some writes are happening.
    let mut tasks = JoinSet::new();
    for i in 0..10u8 {
        let mut cloned_storage = storage.clone();
        tasks.spawn(async move {
            let result = cloned_storage.get(&DbKey(vec![i])).unwrap().unwrap().0[0];
            // The result is either the original value or the new value.
            assert!(result == i || result == i + 10);
        });
    }
    for i in 0..10u8 {
        storage.set(DbKey(vec![i]), DbValue(vec![i + 10])).unwrap();
    }

    tasks.join_all().await;
}
