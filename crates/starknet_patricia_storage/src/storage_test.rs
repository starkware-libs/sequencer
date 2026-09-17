use std::num::NonZeroUsize;
use std::path::Path;

use rstest::rstest;
use tempfile::TempDir;
use tokio::task::JoinSet;

use crate::mdbx_storage::MdbxStorage;
use crate::rocksdb_storage::{RocksDbStorage, RocksDbStorageConfig};
use crate::storage_trait::{AsyncStorage, DbKey, DbValue, ImmutableReadOnlyStorage, Storage};

/// Tests the concurrent access to the storage. Explicitly uses 11 worker threads to get actual
/// parallelism (one thread for main test, 10 worker threads for concurrent operations).
#[rstest]
#[case::rocksdb_storage(
    RocksDbStorage::new(
        Path::new("/tmp/test_rocksdb_storage"), RocksDbStorageConfig::default()
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
            cloned_storage.set(DbKey(vec![i]), DbValue(vec![i])).await.unwrap();
        });
    }

    tasks.join_all().await;

    for i in 0..10u8 {
        assert_eq!(storage.get_mut(&DbKey(vec![i])).await.unwrap(), Some(DbValue(vec![i])));
    }

    // Parallel reads from the storage while some writes are happening.
    let mut tasks = JoinSet::new();
    for i in 0..10u8 {
        let mut cloned_storage = storage.clone();
        tasks.spawn(async move {
            let result = cloned_storage.get_mut(&DbKey(vec![i])).await.unwrap().unwrap().0[0];
            // The result is either the original value or the new value.
            assert!(result == i || result == i + 10);
        });
    }
    for i in 0..10u8 {
        storage.set(DbKey(vec![i]), DbValue(vec![i + 10])).await.unwrap();
    }

    tasks.join_all().await;
}

/// Values must stay aligned with the requested keys however the batch is split.
#[rstest]
#[case::split_across_tasks(32)]
#[case::single_task(1)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_mget_preserves_key_order(#[case] max_read_tasks: usize) {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = RocksDbStorage::new(
        temp_dir.path(),
        RocksDbStorageConfig {
            max_read_tasks: NonZeroUsize::new(max_read_tasks).unwrap(),
            ..Default::default()
        },
    )
    .unwrap();

    const N_KEYS: u32 = 200;
    let value_of = |key: u32| DbValue(key.to_be_bytes().to_vec());
    // Only even keys exist; a misaligned chunk shows up as a `None` in the wrong slot.
    for key in (0..N_KEYS).step_by(2) {
        storage.set(DbKey(key.to_be_bytes().to_vec()), value_of(key)).await.unwrap();
    }

    let keys: Vec<DbKey> = (0..N_KEYS).map(|key| DbKey(key.to_be_bytes().to_vec())).collect();
    let borrowed_keys: Vec<&DbKey> = keys.iter().collect();
    let values = ImmutableReadOnlyStorage::mget(&storage, &borrowed_keys).await.unwrap();

    assert_eq!(values.len(), keys.len());
    for (index, value) in values.iter().enumerate() {
        let key = u32::try_from(index).unwrap();
        let expected = if key % 2 == 0 { Some(value_of(key)) } else { None };
        assert_eq!(*value, expected, "wrong value at index {index}");
    }
}

/// Keys of mixed lengths share one flattened chunk buffer, so a value must still come back for the
/// key it was requested with, whatever the neighboring keys' lengths are.
#[rstest]
#[case::split_across_tasks(32)]
#[case::single_task(1)]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_mget_varying_key_lengths(#[case] max_read_tasks: usize) {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = RocksDbStorage::new(
        temp_dir.path(),
        RocksDbStorageConfig {
            max_read_tasks: NonZeroUsize::new(max_read_tasks).unwrap(),
            ..Default::default()
        },
    )
    .unwrap();

    // Key lengths cycle over 1..=7 bytes so that every chunk mixes lengths, plus one empty key to
    // cover a zero-length span. Repeating the index as the key's bytes keeps the keys distinct.
    const N_KEYS: u8 = 200;
    let key_of = |index: u8| {
        if index == 0 { DbKey(Vec::new()) } else { DbKey(vec![index; usize::from(index) % 7 + 1]) }
    };
    let value_of = |index: u8| DbValue(vec![index; 3]);
    // Only even keys exist; a span read from the wrong offset shows up as a misplaced `None`.
    for index in (0..N_KEYS).step_by(2) {
        storage.set(key_of(index), value_of(index)).await.unwrap();
    }

    let keys: Vec<DbKey> = (0..N_KEYS).map(key_of).collect();
    let borrowed_keys: Vec<&DbKey> = keys.iter().collect();
    let values = ImmutableReadOnlyStorage::mget(&storage, &borrowed_keys).await.unwrap();

    assert_eq!(values.len(), keys.len());
    for (index, value) in values.iter().enumerate() {
        let index = u8::try_from(index).unwrap();
        // The single empty key (index 0) is even, so it is expected to be present.
        let expected = if index % 2 == 0 { Some(value_of(index)) } else { None };
        assert_eq!(*value, expected, "wrong value at index {index}");
    }
}

/// An empty batch must not reach `chunks`, which panics on a zero chunk size. Reachable in
/// production: `CachedStorage::mget` forwards an empty miss list when every key was cached.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_mget_empty_batch() {
    let temp_dir = TempDir::new().unwrap();
    let storage = RocksDbStorage::new(temp_dir.path(), RocksDbStorageConfig::default()).unwrap();
    assert!(ImmutableReadOnlyStorage::mget(&storage, &[]).await.unwrap().is_empty());
}
