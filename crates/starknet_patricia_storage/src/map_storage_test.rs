use std::collections::HashMap;
use std::num::NonZeroUsize;

use rstest::rstest;
use tokio::task::JoinSet;

use crate::map_storage::{CachedStorage, CachedStorageConfig, MapStorage};
use crate::storage_trait::{DbKey, DbValue, Storage};

#[rstest]
#[case::map_storage(MapStorage::default())]
#[case::cached_storage(
    CachedStorage::new(MapStorage::default(), CachedStorageConfig {
        cache_size: NonZeroUsize::new(2).unwrap(),
        cache_on_write: true,
        include_inner_stats: false,
    })
)]
fn test_storage_impl(#[case] mut storage: impl Storage) {
    let (key_1, key_2, key_3) = (DbKey(vec![1_u8]), DbKey(vec![2_u8]), DbKey(vec![3_u8]));
    let (val_1, val_2, val_3) = (DbValue(vec![1_u8]), DbValue(vec![2_u8]), DbValue(vec![3_u8]));

    storage.set(key_1.clone(), val_1.clone()).unwrap();
    // storage = {1: 1}
    assert_eq!(storage.get(&key_1.clone()).unwrap(), Some(val_1.clone()));

    storage.set(key_2.clone(), val_2.clone()).unwrap();
    storage.delete(&key_1).unwrap();
    // storage = {2: 2}
    assert!(storage.get(&key_1.clone()).unwrap().is_none());
    assert_eq!(storage.get(&key_2.clone()).unwrap(), Some(val_2.clone()));

    storage
        .mset(HashMap::from([(key_1.clone(), val_1.clone()), (key_3.clone(), val_3.clone())]))
        .unwrap();
    // storage = {1:1, 2: 2, 3:3}
    assert_eq!(storage.get(&key_2.clone()).unwrap(), Some(val_2.clone()));
    let expected_stored_values = storage.mget(&[&key_1, &key_2, &key_3]).unwrap();
    assert_eq!(
        expected_stored_values,
        vec![Some(val_1.clone()), Some(val_2.clone()), Some(val_3.clone())]
    );

    storage.delete(&key_2).unwrap();
    // storage = {1:1, 3:3}
    assert!(storage.get(&key_2.clone()).unwrap().is_none());
}

/// Tests the concurrent access to the storage. Explicitly uses 11 worker threads to get actual
/// parallelism (one thread for main test, 10 worker threads for concurrent operations).
#[tokio::test(flavor = "multi_thread", worker_threads = 11)]
async fn test_map_storage_concurrent_access() {
    let mut storage = MapStorage::default();

    // Parallel writes to the storage.
    let mut tasks = JoinSet::new();

    for i in 0..10u8 {
        let mut cloned_storage = storage.clone();
        tasks.spawn(async move {
            cloned_storage.set(DbKey(vec![i]), DbValue(vec![i])).unwrap();
        });
    }

    tasks.join_all().await;

    let expected_storage = (0..10u8).map(|i| (DbKey(vec![i]), DbValue(vec![i]))).collect();
    assert_eq!(storage.cloned_map(), expected_storage);

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
