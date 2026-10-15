use std::collections::HashMap;
use std::num::NonZeroUsize;

use rstest::rstest;

use crate::map_storage::{CachedStorage, CachedStorageConfig, MapStorage};
use crate::storage_trait::{DbKey, DbValue, EmptyStorageConfig, Storage};

#[rstest]
#[case::map_storage(MapStorage::default())]
#[case::cached_storage(
    CachedStorage::new(MapStorage::default(), CachedStorageConfig {
        cache_size: NonZeroUsize::new(2).unwrap(),
        include_inner_stats: false,
        inner_storage_config: EmptyStorageConfig::default(),
    })
)]
#[tokio::test]
async fn test_storage_impl(#[case] mut storage: impl Storage) {
    let (key_1, key_2, key_3) = (DbKey(vec![1_u8]), DbKey(vec![2_u8]), DbKey(vec![3_u8]));
    let (val_1, val_2, val_3) = (DbValue(vec![1_u8]), DbValue(vec![2_u8]), DbValue(vec![3_u8]));

    storage.set(key_1.clone(), val_1.clone()).await.unwrap();
    // storage = {1: 1}
    assert_eq!(storage.get_mut(&key_1.clone()).await.unwrap(), Some(val_1.clone()));

    storage.set(key_2.clone(), val_2.clone()).await.unwrap();
    storage.delete(&key_1).await.unwrap();
    // storage = {2: 2}
    assert!(storage.get_mut(&key_1.clone()).await.unwrap().is_none());
    assert_eq!(storage.get_mut(&key_2.clone()).await.unwrap(), Some(val_2.clone()));

    storage
        .mset(HashMap::from([(key_1.clone(), val_1.clone()), (key_3.clone(), val_3.clone())]))
        .await
        .unwrap();
    // storage = {1:1, 2: 2, 3:3}
    assert_eq!(storage.get_mut(&key_2.clone()).await.unwrap(), Some(val_2.clone()));
    let expected_stored_values = storage.mget_mut(&[&key_1, &key_2, &key_3]).await.unwrap();
    assert_eq!(
        expected_stored_values,
        vec![Some(val_1.clone()), Some(val_2.clone()), Some(val_3.clone())]
    );

    storage.delete(&key_2).await.unwrap();
    // storage = {1:1, 3:3}
    assert!(storage.get_mut(&key_2.clone()).await.unwrap().is_none());
}

#[rstest]
#[case::map_storage(MapStorage::default())]
#[case::cached_storage(
    CachedStorage::new(MapStorage::default(), CachedStorageConfig {
        cache_size: NonZeroUsize::new(10).unwrap(),
        include_inner_stats: false,
        inner_storage_config: EmptyStorageConfig::default(),
    })
)]
#[tokio::test]
async fn test_delete_range(#[case] mut storage: impl Storage) {
    let keys = [vec![1], vec![2], vec![2, 5], vec![3], vec![3, 0], vec![4]].map(DbKey);
    for (index, key) in keys.iter().enumerate() {
        storage.set(key.clone(), DbValue(vec![u8::try_from(index).unwrap()])).await.unwrap();
    }
    // Reads populate the cache of a cached storage, so the deletion must evict them too.
    storage.mget_mut(&keys.each_ref()).await.unwrap();

    storage.delete_range(DbKey(vec![2])..=DbKey(vec![3])).await.unwrap();

    let stored: Vec<bool> =
        storage.mget_mut(&keys.each_ref()).await.unwrap().iter().map(Option::is_some).collect();
    assert_eq!(stored, [true, false, false, false, true, true]);
}
