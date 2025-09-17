use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{fs, thread};

use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{fork, ForkResult};
use starknet_api::block::BlockNumber;
use tempfile::tempdir;

use crate::header::HeaderStorageReader;
use crate::test_utils::get_test_config_with_path;
use crate::{open_storage, StorageConfig, StorageError, StorageScope};

#[test]
fn get_storage_twice_should_fail() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config =
        get_test_config_with_path(Some(StorageScope::StateOnly), temp_dir.path().to_path_buf());

    // Get storage first time
    let (reader, mut _writer) = open_storage(config.clone()).unwrap();
    assert!(reader.begin_ro_txn().unwrap().get_block_signature(BlockNumber(0)).unwrap().is_none());

    // Get the same storage second time should fail because tables already exist
    let result = open_storage(config);
    assert!(result.is_err(), "Opening storage twice should fail");
}

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
            thread::sleep(Duration::from_millis(1000));
            open_storage_with_barrier(config, barrier)
        })
    };

    // Wait for both threads to complete
    assert!(handle1.join().unwrap().is_err() || handle2.join().unwrap().is_err());
}

/// Function to handle storage opening with barrier synchronization
fn open_storage_with_barrier(
    config: StorageConfig,
    barrier: Arc<std::sync::Barrier>,
) -> Result<(), StorageError> {
    let result = open_storage(config);
    barrier.wait(); // Synchronize with other thread
    match result {
        Ok((reader, _writer)) => {
            assert!(
                reader
                    .begin_ro_txn()
                    .unwrap()
                    .get_block_signature(BlockNumber(0))
                    .unwrap()
                    .is_none()
            );
            Ok(())
        }
        Err(e) => Err(e),
    }
}

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
        tokio::time::sleep(Duration::from_millis(1000)).await;
        async_open_storage_with_barrier(config, barrier).await
    });

    let results = tokio::try_join!(task1, task2).unwrap();
    assert!(results.0.is_err() || results.1.is_err());
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
            assert!(
                reader
                    .begin_ro_txn()
                    .unwrap()
                    .get_block_signature(BlockNumber(0))
                    .unwrap()
                    .is_none()
            );
            Ok(())
        }
        Err(e) => Err(e),
    }
}

#[test]
fn get_storage_from_two_processes_with_fork_should_fail() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config =
        get_test_config_with_path(Some(StorageScope::StateOnly), temp_dir.path().to_path_buf());

    // Use a signal file for synchronization between processes
    let parent_signal_path = temp_dir.path().join("parent_signal_file");
    let child_signal_path = temp_dir.path().join("child_signal_file");

    match unsafe { fork() } {
        Ok(ForkResult::Parent { child }) => {
            // Parent process
            let (reader, _writer) = open_storage(config.clone()).unwrap();

            // Now parent can finish
            assert!(
                reader
                    .begin_ro_txn()
                    .unwrap()
                    .get_block_signature(BlockNumber(0))
                    .unwrap()
                    .is_none()
            );

            // Create the signal file to notify child that parent is after opening storage.
            fs::write(&parent_signal_path, b"ready").unwrap();

            // Wait for the signal file to be created by the child
            let final_time = Instant::now() + Duration::from_secs(10);
            while !child_signal_path.exists() {
                if Instant::now() >= final_time {
                    panic!("Timeout: Child process did not create signal file within 10 seconds");
                }
                thread::sleep(Duration::from_millis(100));
            }
            // Clean up child signal file
            let _ = fs::remove_file(&child_signal_path);

            // Wait for child process to complete and check its exit status
            match waitpid(child, Some(WaitPidFlag::WUNTRACED)) {
                Ok(WaitStatus::Exited(pid, exit_code)) => {
                    assert!(
                        exit_code == 1,
                        "Parent: Child process (PID: {pid}) should exit with exit code of 1, \
                         received: {exit_code}"
                    );
                }
                Err(e) => {
                    panic!("Parent: Failed to wait for child process: {e}");
                }
                _ => {
                    panic!("Problems with a child process");
                }
            }
        }
        Ok(ForkResult::Child) => {
            // Wait for the signal file to be created by the parent
            let final_time = Instant::now() + Duration::from_secs(10);
            while !parent_signal_path.exists() {
                if Instant::now() >= final_time {
                    println!(
                        "Child: Timeout: Parent process did not create signal file within 10 \
                         seconds"
                    );
                    std::process::exit(2);
                }
                thread::sleep(Duration::from_millis(100));
            }
            // Clean up parent signal file
            let _ = fs::remove_file(&parent_signal_path);

            // Child process - try to open storage directly without catch_unwind
            let result = open_storage(config);
            let mut exit_code = 0;
            match result {
                Ok((reader, _writer)) => {
                    assert!(
                        reader
                            .begin_ro_txn()
                            .unwrap()
                            .get_block_signature(BlockNumber(0))
                            .unwrap()
                            .is_none()
                    );

                    // Create the signal file to notify parent AFTER successful storage opening
                    fs::write(&child_signal_path, b"ready").unwrap();
                }
                Err(e) => {
                    println!("Storage opening failed: {e}");
                    // Create signal file to notify parent of failure
                    fs::write(&child_signal_path, b"error").unwrap();
                    exit_code = 1;
                }
            }
            // Wait a bit to ensure parent has time to check
            thread::sleep(Duration::from_millis(100));

            std::process::exit(exit_code);
        }
        Err(_) => panic!("Fork failed"),
    }
}
