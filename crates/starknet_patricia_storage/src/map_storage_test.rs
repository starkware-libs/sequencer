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
        short_lived_cache_size: None,
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
#[case::map_storage(MapStorage::default(), [true, false, false, false, true, true])]
// The single cache entry holds the in-range key read before freeing, so it stays readable.
#[case::cached_storage(
    CachedStorage::new(MapStorage::default(), CachedStorageConfig {
        cache_size: NonZeroUsize::new(1).unwrap(),
        short_lived_cache_size: None,
        include_inner_stats: false,
        inner_storage_config: EmptyStorageConfig::default(),
    }),
    [true, true, false, false, true, true],
)]
#[tokio::test]
async fn test_free_range(#[case] mut storage: impl Storage, #[case] expected_stored: [bool; 6]) {
    let keys = [vec![1], vec![2], vec![2, 5], vec![3], vec![3, 0], vec![4]].map(DbKey);
    for (index, key) in keys.iter().enumerate() {
        storage.set(key.clone(), DbValue(vec![u8::try_from(index).unwrap()])).await.unwrap();
    }
    storage.get_mut(&keys[1]).await.unwrap();

    storage.free_range(DbKey(vec![2])..=DbKey(vec![3])).await.unwrap();

    let stored: Vec<bool> =
        storage.mget_mut(&keys.each_ref()).await.unwrap().iter().map(Option::is_some).collect();
    assert_eq!(stored, expected_stored);
}

const SHORT_LIVED_KEY_PREFIX: u8 = 9;

fn is_short_lived(key: &DbKey) -> bool {
    key.0[0] == SHORT_LIVED_KEY_PREFIX
}

/// The short-lived keys are cached apart from the others, each cache evicting on its own.
#[tokio::test]
async fn test_short_lived_cache() {
    let mut storage = CachedStorage::new(
        MapStorage::default(),
        CachedStorageConfig {
            cache_size: NonZeroUsize::new(2).unwrap(),
            short_lived_cache_size: Some(NonZeroUsize::new(1).unwrap()),
            include_inner_stats: false,
            inner_storage_config: EmptyStorageConfig::default(),
        },
    )
    .with_short_lived_keys(is_short_lived);
    let key = DbKey(vec![1]);
    let short_lived_key_1 = DbKey(vec![SHORT_LIVED_KEY_PREFIX, 1]);
    let short_lived_key_2 = DbKey(vec![SHORT_LIVED_KEY_PREFIX, 2]);
    for key in [&key, &short_lived_key_1, &short_lived_key_2] {
        storage.set(key.clone(), DbValue(vec![])).await.unwrap();
    }
    assert_eq!(storage.cache.len(), 1);
    assert!(storage.cache.contains(&key));
    let short_lived_cache = storage.short_lived_cache.as_ref().unwrap();
    assert!(!short_lived_cache.contains(&short_lived_key_1));
    assert!(short_lived_cache.contains(&short_lived_key_2));
}
