use std::fs::{self, create_dir_all};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use apollo_storage::db::DbConfig;
use apollo_storage::header::HeaderStorageReader;
use apollo_storage::mmap_file::MmapFileConfig;
use apollo_storage::{open_storage, StorageConfig, StorageError, StorageReader, StorageScope};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{fork, ForkResult};
use starknet_api::block::BlockNumber;
use starknet_api::test_utils::CHAIN_ID_FOR_TESTS;
use tempfile::tempdir;

/// Returns a db config for a given path.
/// This function ensures that the specified directory exists by creating it if necessary.
pub(crate) fn get_test_config_with_path(
    storage_scope: Option<StorageScope>,
    path: PathBuf,
) -> StorageConfig {
    let storage_scope = storage_scope.unwrap_or_default();
    create_dir_all(&path).expect("Failed to create directory");

    StorageConfig {
        db_config: DbConfig {
            path_prefix: path,
            chain_id: CHAIN_ID_FOR_TESTS.clone(),
            enforce_file_exists: false,
            min_size: 1 << 20,    // 1MB
            max_size: 1 << 35,    // 32GB
            growth_step: 1 << 26, // 64MB
            max_readers: 1 << 13, // 8K readers
        },
        scope: storage_scope,
        mmap_file_config: MmapFileConfig {
            max_size: 1 << 24,        // 16MB
            growth_step: 1 << 20,     // 1MB
            max_object_size: 1 << 16, // 64KB
        },
    }
}

/// Check that storage reader can access storage
fn check_storage_is_accessible(reader: &StorageReader) -> bool {
    reader.begin_ro_txn().unwrap().get_block_signature(BlockNumber(0)).unwrap().is_none()
}

/// Helper function to wait for a child process with a timeout.
///
/// Polls `waitpid` with `WNOHANG` flag until either:
/// - The child exits (returns exit status)
/// - The timeout is reached (returns None)
fn waitpid_with_timeout(pid: nix::unistd::Pid, timeout: Duration) -> Option<i32> {
    let start = Instant::now();
    loop {
        match waitpid(pid, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::Exited(_, exit_code)) => return Some(exit_code),
            Ok(WaitStatus::StillAlive) => {
                if start.elapsed() >= timeout {
                    return None;
                }
                thread::sleep(Duration::from_millis(10));
            }
            Ok(_) => return None, // Other status like signaled, stopped, etc.
            Err(_) => return None,
        }
    }
}

/// Test that opening storage from two separate processes fails.
///
/// This test verifies that the libmdbx database exclusive locking mechanism works correctly
/// across process boundaries. When one process has opened the storage with exclusive access,
/// any subsequent attempt by another process to open the same storage should fail.
///
/// # Test Flow
/// 1. **Fork Process**: Creates a parent and child process using `fork()`
/// 2. **Parent Process**:
///    - Opens storage successfully (gets exclusive lock)
///    - Verifies storage is accessible and empty
///    - Signals child process to proceed
///    - Waits for child to complete its attempt
///    - Verifies child process exited with error code 1
/// 3. **Child Process**:
///    - Waits for parent to signal readiness
///    - Attempts to open the same storage (should fail)
///    - Expects `StorageError::InnerError` due to MDBX exclusive lock
///    - Exits with code 1 (indicating expected failure)
///
/// # Synchronization
/// Uses file-based synchronization between processes:
/// - `parent_ready`: Created when parent has opened storage
#[test]
fn get_storage_from_two_processes_with_fork_should_fail() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let config =
        get_test_config_with_path(Some(StorageScope::StateOnly), temp_dir.path().to_path_buf());

    // Use regular files for synchronization between processes
    let parent_ready_path = temp_dir.path().join("parent_ready");

    match unsafe { fork() } {
        Ok(ForkResult::Parent { child }) => {
            // Parent process
            let (reader, _writer) = open_storage(config.clone()).unwrap();
            assert!(check_storage_is_accessible(&reader));

            // Signal child that parent has opened storage
            fs::write(&parent_ready_path, b"1").unwrap();

            // Wait for child process to complete with timeout
            match waitpid_with_timeout(child, Duration::from_secs(5)) {
                Some(exit_code) => {
                    assert!(
                        exit_code == 1,
                        "Parent: Child process should exit with exit code 1, received: {exit_code}"
                    );
                }
                None => {
                    panic!("Parent: Timeout or error waiting for child process to exit");
                }
            }

            // Clean up
            let _ = fs::remove_file(&parent_ready_path);
        }
        Ok(ForkResult::Child) => {
            // Wait for parent to signal that it has opened storage
            let timeout = Instant::now() + Duration::from_secs(5);
            while !parent_ready_path.exists() {
                if Instant::now() >= timeout {
                    println!("Child: Timeout waiting for parent to be ready");
                    std::process::exit(2);
                }
                thread::sleep(Duration::from_millis(10));
            }

            // Child process - try to open storage
            let result = open_storage(config);
            let exit_code = match result {
                Ok((_reader, _writer)) => {
                    println!("Child: Unexpected success opening storage");
                    0
                }
                Err(StorageError::InnerError(_)) => 1,
                Err(e) => {
                    println!("Child: Storage opening failed with unexpected error: {e:?}");
                    2
                }
            };

            std::process::exit(exit_code);
        }
        Err(_) => panic!("Fork failed"),
    }
}
