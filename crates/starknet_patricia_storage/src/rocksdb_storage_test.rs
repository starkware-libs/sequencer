use tempfile::TempDir;

use crate::rocksdb_storage::{RocksDbStorage, RocksDbStorageConfig};
use crate::storage_trait::{DbKey, DbValue, ReadOnlyStorage, Storage};

const N_VALUES_IN_RANGE: u8 = 32;
const VALUE_BYTES: usize = 256 * 1024;

fn key_in_range(index: u8) -> DbKey {
    DbKey(vec![7, index])
}

/// Pseudo-random bytes, so that compression does not shrink the SST files under test.
fn incompressible_value(seed: u8) -> DbValue {
    let mut state = u64::from(seed) + 1;
    DbValue(
        (0..VALUE_BYTES)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state.to_be_bytes()[0]
            })
            .collect(),
    )
}

fn total_sst_files_size(storage: &RocksDbStorage) -> u64 {
    storage.db.property_int_value("rocksdb.total-sst-files-size").unwrap().unwrap()
}

#[tokio::test]
async fn test_delete_range_deletes_exactly_the_range() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage =
        RocksDbStorage::new(temp_dir.path(), RocksDbStorageConfig::default()).unwrap();
    let keys = [vec![1], vec![2], vec![2, 5], vec![3], vec![3, 0], vec![4]].map(DbKey);
    for (index, key) in keys.iter().enumerate() {
        storage.set(key.clone(), DbValue(vec![u8::try_from(index).unwrap()])).await.unwrap();
    }

    storage.delete_range(DbKey(vec![2])..=DbKey(vec![3])).await.unwrap();

    let stored: Vec<bool> =
        storage.mget_mut(&keys.each_ref()).await.unwrap().iter().map(Option::is_some).collect();
    assert_eq!(stored, [true, false, false, false, true, true]);
}

#[tokio::test]
async fn test_delete_range_frees_files_inside_the_range_without_compaction() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage =
        RocksDbStorage::new(temp_dir.path(), RocksDbStorageConfig::default()).unwrap();
    for index in 0..N_VALUES_IN_RANGE {
        storage.set(key_in_range(index), incompressible_value(index)).await.unwrap();
    }
    storage.db.flush().unwrap();
    storage.db.compact_range::<&[u8], &[u8]>(None, None);
    let key_outside_range = DbKey(vec![8]);
    storage.set(key_outside_range.clone(), incompressible_value(N_VALUES_IN_RANGE)).await.unwrap();
    storage.db.flush().unwrap();
    let bytes_in_range = u64::from(N_VALUES_IN_RANGE) * u64::try_from(VALUE_BYTES).unwrap();
    assert!(total_sst_files_size(&storage) > bytes_in_range);

    storage.delete_range(key_in_range(0)..=key_in_range(N_VALUES_IN_RANGE - 1)).await.unwrap();

    assert!(total_sst_files_size(&storage) < bytes_in_range / 10);
    assert_eq!(storage.get_mut(&key_in_range(0)).await.unwrap(), None);
    assert_eq!(
        storage.get_mut(&key_outside_range).await.unwrap(),
        Some(incompressible_value(N_VALUES_IN_RANGE))
    );
}
