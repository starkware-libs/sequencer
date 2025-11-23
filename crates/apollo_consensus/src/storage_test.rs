use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use apollo_storage::db::DbConfig;
use apollo_storage::StorageConfig;
use assert_matches::assert_matches;
use starknet_api::block::BlockNumber;

use crate::storage::{get_voted_height_storage, HeightVotedStorageError, HeightVotedStorageTrait};

/// Returns a config for a new (i.e. empty) storage.
fn get_new_storage_config() -> StorageConfig {
    static DB_INDEX: AtomicUsize = AtomicUsize::new(0);
    let db_file_path = format!(
        "{}-{}",
        tempfile::tempdir().unwrap().path().to_str().unwrap(),
        DB_INDEX.fetch_add(1, Ordering::Relaxed)
    );
    StorageConfig {
        db_config: DbConfig { path_prefix: PathBuf::from(db_file_path), ..Default::default() },
        ..Default::default()
    }
}

#[test]
fn read_last_height_when_no_last_height_in_storage() {
    let storage = get_voted_height_storage(get_new_storage_config());
    assert!(storage.get_prev_voted_height().unwrap().is_none());
}

#[test]
fn read_last_height_when_existing_last_height_in_storage() {
    let mut storage = get_voted_height_storage(get_new_storage_config());
    storage.set_prev_voted_height(BlockNumber(1)).unwrap();
    assert_eq!(storage.get_prev_voted_height().unwrap(), Some(BlockNumber(1)));
}

#[test]
fn write_last_height_when_no_last_height_in_storage() {
    let mut storage = get_voted_height_storage(get_new_storage_config());
    assert!(storage.get_prev_voted_height().unwrap().is_none());
    storage.set_prev_voted_height(BlockNumber(1)).unwrap();
    assert_eq!(storage.get_prev_voted_height().unwrap(), Some(BlockNumber(1)));
}

#[test]
fn write_last_height_when_previous_last_height_in_storage() {
    let mut storage = get_voted_height_storage(get_new_storage_config());
    storage.set_prev_voted_height(BlockNumber(1)).unwrap();
    assert_eq!(storage.get_prev_voted_height().unwrap(), Some(BlockNumber(1)));
    storage.set_prev_voted_height(BlockNumber(2)).unwrap();
    assert_eq!(storage.get_prev_voted_height().unwrap(), Some(BlockNumber(2)));
}

#[test]
fn write_last_height_return_error_when_previous_last_height_is_equal() {
    let mut storage = get_voted_height_storage(get_new_storage_config());
    storage.set_prev_voted_height(BlockNumber(2)).unwrap();
    assert_eq!(storage.get_prev_voted_height().unwrap(), Some(BlockNumber(2)));
    assert_matches!(
        storage.set_prev_voted_height(BlockNumber(1)),
        Err(HeightVotedStorageError::InconsistentStorageState { error_msg: _ })
    );
}
