use std::collections::HashMap;
use std::num::NonZeroUsize;

use rstest::rstest;

use crate::map_storage::{CachedStorage, CachedStorageConfig, MapStorage};
use crate::storage_trait::{DbKey, DbValue, Storage};

#[rstest]
#[case::map_storage(MapStorage::default())]
#[case::cached_storage(
    CachedStorage::new(MapStorage::default(), CachedStorageConfig {
        cache_size: NonZeroUsize::new(2).unwrap(),
        cache_on_write: true,
    })
)]
fn test_storage_impl(#[case] mut storage: impl Storage) {
    let (key_1, key_2, key_3) = (DbKey(vec![1_u8]), DbKey(vec![2_u8]), DbKey(vec![3_u8]));
    let (val_1, val_2, val_3) = (DbValue(vec![1_u8]), DbValue(vec![2_u8]), DbValue(vec![3_u8]));

    storage.set(key_1.clone().into(), val_1.clone()).unwrap();
    // storage = {1: 1}
    assert_eq!(storage.get(&key_1.clone().into()).unwrap(), Some(val_1.clone()));

    storage.set(key_2.clone().into(), val_2.clone()).unwrap();
    storage.delete(&key_1.clone().into()).unwrap();
    // storage = {2: 2}
    assert!(storage.get(&key_1.clone().into()).unwrap().is_none());
    assert_eq!(storage.get(&key_2.clone().into()).unwrap(), Some(val_2.clone()));

    storage
        .mset(HashMap::from([
            (key_1.clone().into(), val_1.clone()),
            (key_3.clone().into(), val_3.clone()),
        ]))
        .unwrap();
    // storage = {1:1, 2: 2, 3:3}
    assert_eq!(storage.get(&key_2.clone().into()).unwrap(), Some(val_2.clone()));
    let expected_stored_values = storage
        .mget(&[&key_1.clone().into(), &key_2.clone().into(), &key_3.clone().into()])
        .unwrap();
    assert_eq!(
        expected_stored_values,
        vec![Some(val_1.clone()), Some(val_2.clone()), Some(val_3.clone())]
    );

    storage.delete(&key_2.clone().into()).unwrap();
    // storage = {1:1, 3:3}
    assert!(storage.get(&key_2.clone().into()).unwrap().is_none());
}
