use std::sync::Arc;
use std::thread;
use std::time::Duration;

use starknet_api::block::BlockNumber;
use tempfile::tempdir;

use crate::header::HeaderStorageReader;
use crate::test_utils::get_test_config_with_path;
use crate::{open_storage, StorageConfig, StorageError, StorageReader, StorageScope};

/// Check that storage reader can access storage
fn check_storage_is_accessible(reader: &StorageReader) -> bool {
    reader.begin_ro_txn().unwrap().get_block_signature(BlockNumber(0)).unwrap().is_none()
}

/// Test that opening storage twice in the same thread fails.
///
/// This test verifies that attempting to open the same storage database twice
/// within a single thread results in an libmdbx error.
///
/// # Test Flow
/// 1. Opens storage successfully the first time
/// 2. Verifies the storage is accessible and empty
/// 3. Attempts to open the same storage again (should fail)
/// 4. Asserts that the second attempt returns `StorageError::InnerError`
#[test]
fn get_storage_twice_should_fail() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config =
        get_test_config_with_path(Some(StorageScope::StateOnly), temp_dir.path().to_path_buf());

    // Get storage first time
    let (reader, mut _writer) = open_storage(config.clone()).unwrap();
    assert!(check_storage_is_accessible(&reader));

    // Get the same storage second time should fail because tables already exist
    let result = open_storage(config);
    assert!(
        matches!(result, Err(StorageError::InnerError(_))),
        "Opening storage twice should fail"
    );
}

/// Test that opening storage from two threads fails.
///
/// This test verifies that when two threads attempt to open the same storage
/// database concurrently, only one succeeds while the other fails with an error.
/// Uses thread synchronization via barriers to ensure both threads attempt
/// storage access simultaneously.
///
/// # Test Flow
/// 1. Creates two threads that will attempt to open storage
/// 2. Uses `std::sync::Barrier` to synchronize thread execution
/// 3. First thread opens storage immediately
/// 4. Second thread waits 100 milliseconds then attempts to open storage
/// 5. Both threads synchronize at barrier after their attempts
/// 6. Verifies that first thread succeeds and second thread fails
#[test]
fn get_storage_from_two_threads_should_fail() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config =
        get_test_config_with_path(Some(StorageScope::StateOnly), temp_dir.path().to_path_buf());
    let barrier = Arc::new(std::sync::Barrier::new(2));

    // Start both threads
    let config1 = config.clone();
    let barrier1 = barrier.clone();
    let handle1 = thread::spawn(move || open_storage_with_barrier(config1, barrier1));

    let handle2 = {
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            open_storage_with_barrier(config, barrier)
        })
    };

    // Wait for both threads to complete
    let result1 = handle1.join().unwrap();
    let result2 = handle2.join().unwrap();
    assert!(
        result1.is_ok() && matches!(result2, Err(StorageError::InnerError(_))),
        "Opening storage from two threads should fail"
    );
}

/// Function to handle storage opening with barrier synchronization.
fn open_storage_with_barrier(
    config: StorageConfig,
    barrier: Arc<std::sync::Barrier>,
) -> Result<(), StorageError> {
    let result = open_storage(config);
    barrier.wait(); // Synchronize with other thread.
    match result {
        Ok((reader, _writer)) => {
            assert!(check_storage_is_accessible(&reader));
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Test that opening storage from two async tokio tasks fails.
///
/// This test verifies that when two async tasks attempt to open the same storage
/// database concurrently, only one succeeds while the other fails with an error.
/// Uses tokio's async barrier synchronization to coordinate task execution.
///
/// # Test Flow
/// 1. Creates two async tasks that will attempt to open storage
/// 2. Uses `tokio::sync::Barrier` to synchronize task execution
/// 3. First task opens storage immediately
/// 4. Second task waits 100 milliseconds then attempts to open storage
/// 5. Both tasks synchronize at barrier after their attempts
/// 6. Verifies that first task succeeds and second task fails
#[tokio::test]
async fn get_storage_from_two_tokio_tasks_should_fail() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config =
        get_test_config_with_path(Some(StorageScope::StateOnly), temp_dir.path().to_path_buf());

    let barrier = Arc::new(tokio::sync::Barrier::new(2));

    let config1 = config.clone();
    let barrier1 = barrier.clone();
    let task1 =
        tokio::spawn(async move { async_open_storage_with_barrier(config1, barrier1).await });

    let task2 = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        async_open_storage_with_barrier(config, barrier).await
    });

    let results = tokio::join!(task1, task2);

    let task1_result = results.0.unwrap();
    let task2_result = results.1.unwrap();
    assert!(
        task1_result.is_ok() && matches!(task2_result, Err(StorageError::InnerError(_))),
        "Opening storage from two tokio tasks should fail"
    );
}

/// Function to handle storage opening with barrier synchronization
async fn async_open_storage_with_barrier(
    config: StorageConfig,
    barrier: Arc<tokio::sync::Barrier>,
) -> Result<(), StorageError> {
    let result = open_storage(config);
    barrier.wait().await; // Synchronize with other thread
    match result {
        Ok((reader, _writer)) => {
            assert!(check_storage_is_accessible(&reader));
            Ok(())
        }
        Err(e) => Err(e),
    }
}
