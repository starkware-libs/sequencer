#![allow(clippy::as_conversions)]

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use starknet_api::block::{BlockHash, BlockHeader, BlockNumber};
use starknet_api::felt;
use tempfile::tempdir;

use crate::header::{HeaderStorageReader, HeaderStorageWriter};
use crate::test_utils::get_test_config_with_path;
use crate::{BatchConfig, StorageConfig, StorageError, StorageReader, StorageScope, open_storage};

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

// Batching Tests:

/// Helper to create a test block header with a unique hash for batching tests.
fn create_test_header_for_batching(block_number: BlockNumber) -> BlockHeader {
    // Create a unique block hash for each block number to avoid KeyAlreadyExists errors.
    BlockHeader {
        block_hash: BlockHash(felt!(block_number.0)),
        block_header_without_hash: starknet_api::block::BlockHeaderWithoutHash {
            block_number,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Helper to create storage with custom batch config for batching tests.
///
/// Note: We first initialize the storage without batching to ensure version info is written,
/// then close and reopen with batching enabled. This is necessary because the initial
/// version setup commits need to be persisted immediately.
fn create_storage_with_batch_config(
    batch_config: BatchConfig,
) -> ((crate::StorageReader, crate::StorageWriter), tempfile::TempDir) {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let mut config =
        get_test_config_with_path(Some(StorageScope::FullArchive), temp_dir.path().to_path_buf());

    // First, open storage without batching to initialize version info.
    config.batch_config = BatchConfig { enabled: false, batch_size: 100 };
    let (_reader, _writer) =
        open_storage(config.clone()).expect("Failed to open storage for initialization");

    // Drop the first reader/writer to close the storage.
    drop(_reader);
    drop(_writer);

    // Now reopen with the desired batch config.
    config.batch_config = batch_config;
    let (reader, mut writer) = open_storage(config).expect("Failed to open storage with batching");

    // IMPORTANT: Reset batch counter after storage initialization.
    // When storage opens, set_version_if_needed() is called which does an empty commit
    // (no version update needed). This empty commit increments the batch counter.
    // For test predictability, we reset the counter here so tests can expect exact
    // batch sizes without accounting for the initialization empty commit.
    writer.reset_batch_counter_for_testing();

    ((reader, writer), temp_dir)
}

/// Test that when batching is disabled, each commit writes to the database immediately.
///
/// This test verifies that with batching disabled, writes are immediately visible
/// to readers after commit, without waiting for a batch threshold.
///
/// # Test Flow
/// 1. Creates storage with batching disabled
/// 2. Writes and commits a single header
/// 3. Verifies the header is immediately visible to the reader
#[test]
fn test_batching_disabled_commits_immediately() {
    let ((reader, mut writer), _temp_dir) =
        create_storage_with_batch_config(BatchConfig { enabled: false, batch_size: 100 });

    // Write and commit a header.
    writer
        .begin_rw_txn()
        .expect("Failed to begin transaction")
        .append_header(BlockNumber(0), &create_test_header_for_batching(BlockNumber(0)))
        .expect("Failed to append header")
        .commit()
        .expect("Failed to commit");

    // Verify it's immediately visible.
    let header = reader
        .begin_ro_txn()
        .expect("Failed to begin read transaction")
        .get_block_header(BlockNumber(0))
        .expect("Failed to get header");

    assert!(header.is_some(), "Header should be immediately visible when batching is disabled");
    assert_eq!(header.unwrap().block_header_without_hash.block_number, BlockNumber(0));
}

/// Test that when batching is enabled, commits are delayed until batch_size is reached.
///
/// This test verifies that with batching enabled, writes are not visible to readers
/// until the batch size threshold is reached and the batch commits.
///
/// # Test Flow
/// 1. Creates storage with batching enabled (batch_size = 5)
/// 2. Writes batch_size - 1 headers
/// 3. Verifies none of the headers are visible yet
/// 4. Writes one more header to reach batch_size
/// 5. Verifies all headers are now visible after batch commit
#[test]
fn test_batching_enabled_delays_commit() {
    let batch_size = 5;
    let ((reader, mut writer), _temp_dir) =
        create_storage_with_batch_config(BatchConfig { enabled: true, batch_size });

    // Write batch_size - 1 headers.
    for i in 0..(batch_size - 1) {
        writer
            .begin_rw_txn()
            .expect("Failed to begin transaction")
            .append_header(
                BlockNumber(i as u64),
                &create_test_header_for_batching(BlockNumber(i as u64)),
            )
            .expect("Failed to append header")
            .commit()
            .expect("Failed to commit");
    }

    // Verify headers are not visible yet (batch not committed).
    for i in 0..(batch_size - 1) {
        let header = reader
            .begin_ro_txn()
            .expect("Failed to begin read transaction")
            .get_block_header(BlockNumber(i as u64))
            .expect("Failed to get header");
        assert!(header.is_none(), "Header {} should not be visible before batch commit", i);
    }

    // Write one more header to reach batch_size.
    writer
        .begin_rw_txn()
        .expect("Failed to begin transaction")
        .append_header(
            BlockNumber((batch_size - 1) as u64),
            &create_test_header_for_batching(BlockNumber((batch_size - 1) as u64)),
        )
        .expect("Failed to append header")
        .commit()
        .expect("Failed to commit");

    // Now all headers should be visible.
    for i in 0..batch_size {
        let header = reader
            .begin_ro_txn()
            .expect("Failed to begin read transaction")
            .get_block_header(BlockNumber(i as u64))
            .expect("Failed to get header");
        assert!(header.is_some(), "Header {} should be visible after batch commit", i);
        assert_eq!(header.unwrap().block_header_without_hash.block_number, BlockNumber(i as u64));
    }
}

/// Test that committing exactly batch_size operations triggers a commit.
///
/// This test verifies that when exactly batch_size commits occur,
/// the batch is flushed and all data becomes visible.
///
/// # Test Flow
/// 1. Creates storage with batch_size = 3
/// 2. Writes exactly 3 headers
/// 3. Verifies all headers are visible after the third commit
#[test]
fn test_batching_exact_batch_size() {
    let batch_size = 3;
    let ((reader, mut writer), _temp_dir) =
        create_storage_with_batch_config(BatchConfig { enabled: true, batch_size });

    // Write exactly batch_size headers.
    for i in 0..batch_size {
        writer
            .begin_rw_txn()
            .expect("Failed to begin transaction")
            .append_header(
                BlockNumber(i as u64),
                &create_test_header_for_batching(BlockNumber(i as u64)),
            )
            .expect("Failed to append header")
            .commit()
            .expect("Failed to commit");
    }

    // All headers should be visible.
    for i in 0..batch_size {
        let header = reader
            .begin_ro_txn()
            .expect("Failed to begin read transaction")
            .get_block_header(BlockNumber(i as u64))
            .expect("Failed to get header");
        assert!(header.is_some(), "Header {} should be visible", i);
    }
}

/// Test that multiple batches work correctly.
///
/// This test verifies that the batching mechanism correctly handles multiple
/// complete batches, with the counter resetting after each batch commit.
///
/// # Test Flow
/// 1. Creates storage with batch_size = 3
/// 2. Writes 3 complete batches (9 headers total)
/// 3. After each batch, verifies all headers up to that point are visible
/// 4. Ensures counter resets properly between batches
#[test]
fn test_batching_multiple_batches() {
    let batch_size = 3;
    let num_batches = 3;
    let ((reader, mut writer), _temp_dir) =
        create_storage_with_batch_config(BatchConfig { enabled: true, batch_size });

    // Write multiple complete batches.
    for batch in 0..num_batches {
        for i in 0..batch_size {
            let block_num = (batch * batch_size + i) as u64;
            writer
                .begin_rw_txn()
                .expect("Failed to begin transaction")
                .append_header(
                    BlockNumber(block_num),
                    &create_test_header_for_batching(BlockNumber(block_num)),
                )
                .expect("Failed to append header")
                .commit()
                .expect("Failed to commit");
        }

        // After each batch, all headers up to this point should be visible.
        for j in 0..=batch {
            for i in 0..batch_size {
                let block_num = (j * batch_size + i) as u64;
                let header = reader
                    .begin_ro_txn()
                    .expect("Failed to begin read transaction")
                    .get_block_header(BlockNumber(block_num))
                    .expect("Failed to get header");
                assert!(
                    header.is_some(),
                    "Header {} should be visible after batch {} commit",
                    block_num,
                    batch
                );
            }
        }
    }
}

/// Test that an incomplete batch (less than batch_size commits) is not visible.
///
/// This test verifies that writes remain invisible to readers when the number
/// of commits is less than batch_size.
///
/// # Test Flow
/// 1. Creates storage with batch_size = 10
/// 2. Writes only 5 headers (half a batch)
/// 3. Verifies none of the headers are visible to the reader
#[test]
fn test_batching_incomplete_batch_not_visible() {
    let batch_size = 10;
    let ((reader, mut writer), _temp_dir) =
        create_storage_with_batch_config(BatchConfig { enabled: true, batch_size });

    // Write only half a batch.
    let half_batch = batch_size / 2;
    for i in 0..half_batch {
        writer
            .begin_rw_txn()
            .expect("Failed to begin transaction")
            .append_header(
                BlockNumber(i as u64),
                &create_test_header_for_batching(BlockNumber(i as u64)),
            )
            .expect("Failed to append header")
            .commit()
            .expect("Failed to commit");
    }

    // None of the headers should be visible.
    for i in 0..half_batch {
        let header = reader
            .begin_ro_txn()
            .expect("Failed to begin read transaction")
            .get_block_header(BlockNumber(i as u64))
            .expect("Failed to get header");
        assert!(header.is_none(), "Header {} should not be visible in incomplete batch", i);
    }
}

/// Test edge case: batch_size = 1 should commit every operation.
///
/// This test verifies that when batch_size is set to 1, each commit
/// immediately flushes to the database, effectively disabling batching.
///
/// # Test Flow
/// 1. Creates storage with batch_size = 1
/// 2. Writes 5 headers sequentially
/// 3. After each commit, verifies the header is immediately visible
#[test]
fn test_batching_batch_size_one() {
    let ((reader, mut writer), _temp_dir) =
        create_storage_with_batch_config(BatchConfig { enabled: true, batch_size: 1 });

    // Each commit should be immediately visible.
    for i in 0..5 {
        writer
            .begin_rw_txn()
            .expect("Failed to begin transaction")
            .append_header(BlockNumber(i), &create_test_header_for_batching(BlockNumber(i)))
            .expect("Failed to append header")
            .commit()
            .expect("Failed to commit");

        // Should be immediately visible.
        let header = reader
            .begin_ro_txn()
            .expect("Failed to begin read transaction")
            .get_block_header(BlockNumber(i))
            .expect("Failed to get header");
        assert!(header.is_some(), "Header {} should be immediately visible with batch_size=1", i);
    }
}

/// Test with a large batch_size to ensure counter works correctly.
///
/// This test verifies that the batching mechanism handles large batch sizes
/// correctly and that the counter accurately tracks commits up to the threshold.
///
/// # Test Flow
/// 1. Creates storage with batch_size = 100
/// 2. Writes 99 headers (one less than batch_size)
/// 3. Verifies headers are not visible yet
/// 4. Writes one more header to complete the batch
/// 5. Verifies all headers are now visible
#[test]
fn test_batching_large_batch_size() {
    let batch_size = 100;
    let ((reader, mut writer), _temp_dir) =
        create_storage_with_batch_config(BatchConfig { enabled: true, batch_size });

    // Write batch_size - 1 headers.
    for i in 0..(batch_size - 1) {
        writer
            .begin_rw_txn()
            .expect("Failed to begin transaction")
            .append_header(
                BlockNumber(i as u64),
                &create_test_header_for_batching(BlockNumber(i as u64)),
            )
            .expect("Failed to append header")
            .commit()
            .expect("Failed to commit");
    }

    // Should not be visible yet.
    let header = reader
        .begin_ro_txn()
        .expect("Failed to begin read transaction")
        .get_block_header(BlockNumber(0))
        .expect("Failed to get header");
    assert!(header.is_none(), "Headers should not be visible before batch commit");

    // Write one more to complete the batch.
    writer
        .begin_rw_txn()
        .expect("Failed to begin transaction")
        .append_header(
            BlockNumber((batch_size - 1) as u64),
            &create_test_header_for_batching(BlockNumber((batch_size - 1) as u64)),
        )
        .expect("Failed to append header")
        .commit()
        .expect("Failed to commit");

    // Now should be visible.
    let header = reader
        .begin_ro_txn()
        .expect("Failed to begin read transaction")
        .get_block_header(BlockNumber(0))
        .expect("Failed to get header");
    assert!(header.is_some(), "Headers should be visible after batch commit");
}

/// Test that the commit counter resets correctly after a batch commit.
///
/// This test verifies that after a batch is flushed, the counter resets to zero
/// and correctly counts commits for the next batch.
///
/// # Test Flow
/// 1. Creates storage with batch_size = 3
/// 2. Writes first batch of 3 headers (triggers commit)
/// 3. Verifies first batch is visible
/// 4. Writes 2 headers for second batch (incomplete)
/// 5. Verifies second batch is not visible yet (counter reset correctly)
/// 6. Completes second batch with one more header
/// 7. Verifies second batch is now visible
#[test]
fn test_batching_counter_resets_after_commit() {
    let batch_size = 3;
    let ((reader, mut writer), _temp_dir) =
        create_storage_with_batch_config(BatchConfig { enabled: true, batch_size });

    // First batch.
    for i in 0..batch_size {
        writer
            .begin_rw_txn()
            .expect("Failed to begin transaction")
            .append_header(
                BlockNumber(i as u64),
                &create_test_header_for_batching(BlockNumber(i as u64)),
            )
            .expect("Failed to append header")
            .commit()
            .expect("Failed to commit");
    }

    // Verify first batch is visible
    let header = reader
        .begin_ro_txn()
        .expect("Failed to begin read transaction")
        .get_block_header(BlockNumber(0))
        .expect("Failed to get header");
    assert!(header.is_some(), "First batch should be visible");

    // Second batch - only batch_size - 1 commits
    for i in 0..(batch_size - 1) {
        let block_num = (batch_size + i) as u64;
        writer
            .begin_rw_txn()
            .expect("Failed to begin transaction")
            .append_header(
                BlockNumber(block_num),
                &create_test_header_for_batching(BlockNumber(block_num)),
            )
            .expect("Failed to append header")
            .commit()
            .expect("Failed to commit");
    }

    // Second batch should NOT be visible yet (counter reset correctly)
    let header = reader
        .begin_ro_txn()
        .expect("Failed to begin read transaction")
        .get_block_header(BlockNumber(batch_size as u64))
        .expect("Failed to get header");
    assert!(header.is_none(), "Incomplete second batch should not be visible");

    // Complete second batch
    writer
        .begin_rw_txn()
        .expect("Failed to begin transaction")
        .append_header(
            BlockNumber((2 * batch_size - 1) as u64),
            &create_test_header_for_batching(BlockNumber((2 * batch_size - 1) as u64)),
        )
        .expect("Failed to append header")
        .commit()
        .expect("Failed to commit");

    // Now second batch should be visible
    let header = reader
        .begin_ro_txn()
        .expect("Failed to begin read transaction")
        .get_block_header(BlockNumber(batch_size as u64))
        .expect("Failed to get header");
    assert!(header.is_some(), "Second batch should be visible after commit");
}

/// Test that the same persistent transaction is reused across multiple begin_rw_txn calls.
///
/// This test verifies that with batching enabled, multiple calls to begin_rw_txn
/// reuse the same underlying persistent transaction rather than creating new ones.
///
/// # Test Flow
/// 1. Creates storage with batching enabled
/// 2. Calls begin_rw_txn multiple times without committing
/// 3. Writes and commits headers across multiple transactions
/// 4. Verifies operations succeed without errors (persistent transaction stays alive)
#[test]
fn test_batching_persistent_transaction_reuse() {
    let batch_size = 5;
    let ((_reader, mut writer), _temp_dir) =
        create_storage_with_batch_config(BatchConfig { enabled: true, batch_size });

    // Multiple begin_rw_txn calls should reuse the same underlying transaction
    for _i in 0..batch_size {
        let _txn = writer.begin_rw_txn().expect("Failed to begin transaction");
        // Transaction is dropped here without commit
        // This tests that the persistent transaction stays alive
    }

    // Now actually write and commit
    for _i in 0..batch_size {
        writer
            .begin_rw_txn()
            .expect("Failed to begin transaction")
            .append_header(
                BlockNumber(_i as u64),
                &create_test_header_for_batching(BlockNumber(_i as u64)),
            )
            .expect("Failed to append header")
            .commit()
            .expect("Failed to commit");
    }

    // Should succeed without errors.
}

/// Test that data written within a batch is consistent when read back.
///
/// This test verifies that all data written within a batch is correctly
/// persisted and matches the original data when read after batch commit.
///
/// # Test Flow
/// 1. Creates storage with batch_size = 3
/// 2. Writes 3 headers to complete a batch
/// 3. Reads back all headers
/// 4. Verifies each header matches the original data exactly
#[test]
fn test_batching_data_consistency_within_batch() {
    let batch_size = 3;
    let ((reader, mut writer), _temp_dir) =
        create_storage_with_batch_config(BatchConfig { enabled: true, batch_size });

    // Write headers.
    for i in 0..batch_size {
        writer
            .begin_rw_txn()
            .expect("Failed to begin transaction")
            .append_header(
                BlockNumber(i as u64),
                &create_test_header_for_batching(BlockNumber(i as u64)),
            )
            .expect("Failed to append header")
            .commit()
            .expect("Failed to commit");
    }

    // Verify all headers exist and match.
    for i in 0..batch_size {
        let actual_header = reader
            .begin_ro_txn()
            .expect("Failed to begin read transaction")
            .get_block_header(BlockNumber(i as u64))
            .expect("Failed to get header")
            .expect("Header should exist");

        // Verify the header is the default header (what we wrote).
        assert_eq!(
            actual_header,
            create_test_header_for_batching(BlockNumber(i as u64)),
            "Header mismatch for block {}",
            i
        );
    }
}

/// Test edge case: batch_size = 0 should behave like disabled batching.
///
/// This test verifies the behavior when batch_size is set to 0. Currently,
/// it should behave like batching is disabled, committing immediately.
///
/// # Test Flow
/// 1. Creates storage with batch_size = 0
/// 2. Writes and commits a header
/// 3. Verifies the header is immediately visible
///
/// # Note
/// This tests the current behavior. If batch_size=0 should panic or error,
/// this test should be updated accordingly.
#[test]
fn test_batching_zero_batch_size_treated_as_disabled() {
    let ((reader, mut writer), _temp_dir) =
        create_storage_with_batch_config(BatchConfig { enabled: true, batch_size: 0 });

    // With batch_size=0, the first commit should trigger immediate commit.
    writer
        .begin_rw_txn()
        .expect("Failed to begin transaction")
        .append_header(BlockNumber(0), &create_test_header_for_batching(BlockNumber(0)))
        .expect("Failed to append header")
        .commit()
        .expect("Failed to commit");

    // Should be immediately visible.
    let header = reader
        .begin_ro_txn()
        .expect("Failed to begin read transaction")
        .get_block_header(BlockNumber(0))
        .expect("Failed to get header");

    assert!(
        header.is_some(),
        "With batch_size=0, commits should happen immediately (or this should be an error)"
    );
}

/// Test that within a single begin_rw_txn() call, we can read what we just wrote.
///
/// This test verifies that within a single transaction, writes are immediately
/// visible to subsequent reads in the same transaction (read-your-own-writes).
///
/// # Test Flow
/// 1. Creates storage with batching enabled
/// 2. Within one transaction, writes a header and immediately reads it back
/// 3. Verifies the header is visible within the same transaction
#[test]
fn test_read_own_uncommitted_write_same_transaction() {
    let ((_reader, mut writer), _temp_dir) =
        create_storage_with_batch_config(BatchConfig { enabled: true, batch_size: 10 });

    // Write and read within the same transaction.
    let header = writer
        .begin_rw_txn()
        .expect("Failed to begin transaction")
        .append_header(BlockNumber(0), &create_test_header_for_batching(BlockNumber(0)))
        .expect("Failed to append header")
        .get_block_header(BlockNumber(0))
        .expect("Failed to get header");

    // Should be able to read what we just wrote in the same transaction.
    assert!(header.is_some(), "Should be able to read own write in same transaction");
    assert_eq!(header.unwrap().block_header_without_hash.block_number, BlockNumber(0));
}

/// Test that across multiple begin_rw_txn() calls, we can read what we wrote in previous calls.
///
/// This test verifies that with batching enabled, multiple begin_rw_txn() calls
/// share the same persistent transaction, allowing the writer to read uncommitted
/// writes from previous transactions.
///
/// # Test Flow
/// 1. Creates storage with batching enabled
/// 2. First transaction writes a header and commits
/// 3. Second transaction reads the header written in the first transaction
/// 4. Verifies the header is visible (same persistent transaction)
#[test]
fn test_read_own_uncommitted_write_across_transactions() {
    let ((_reader, mut writer), _temp_dir) =
        create_storage_with_batch_config(BatchConfig { enabled: true, batch_size: 10 });

    // First transaction: write header 0.
    writer
        .begin_rw_txn()
        .expect("Failed to begin transaction")
        .append_header(BlockNumber(0), &create_test_header_for_batching(BlockNumber(0)))
        .expect("Failed to append header")
        .commit()
        .expect("Failed to commit");

    // Second transaction: read header 0 (should see it because same persistent txn).
    let header = writer
        .begin_rw_txn()
        .expect("Failed to begin transaction")
        .get_block_header(BlockNumber(0))
        .expect("Failed to get header");

    assert!(
        header.is_some(),
        "Should be able to read write from previous begin_rw_txn() call (same persistent txn)"
    );
    assert_eq!(header.unwrap().block_header_without_hash.block_number, BlockNumber(0));
}

/// Test the classic read-modify-write pattern within batching.
///
/// This test verifies that the read-modify-write pattern works correctly
/// with batching, where operations need to read existing data before writing.
///
/// # Test Flow
/// 1. Creates storage with batching enabled
/// 2. Writes initial header (block 0)
/// 3. Reads the existing header, then writes a new header (block 1)
/// 4. Verifies both headers are readable in subsequent transaction
#[test]
fn test_read_modify_write_pattern() {
    let ((_reader, mut writer), _temp_dir) =
        create_storage_with_batch_config(BatchConfig { enabled: true, batch_size: 10 });

    // Step 1: Write initial header.
    writer
        .begin_rw_txn()
        .expect("Failed to begin transaction")
        .append_header(BlockNumber(0), &create_test_header_for_batching(BlockNumber(0)))
        .expect("Failed to append header")
        .commit()
        .expect("Failed to commit");

    // Step 2: Read the header, "modify" it (write a different block), and write again.
    let txn = writer.begin_rw_txn().expect("Failed to begin transaction");

    // Read the existing header.
    let existing_header = txn.get_block_header(BlockNumber(0)).expect("Failed to get header");
    assert!(existing_header.is_some(), "Should be able to read previously written header");

    // Write a new header (simulating a modification).
    txn.append_header(BlockNumber(1), &create_test_header_for_batching(BlockNumber(1)))
        .expect("Failed to append header")
        .commit()
        .expect("Failed to commit");

    // Step 3: Verify both headers are readable.
    let txn = writer.begin_rw_txn().expect("Failed to begin transaction");

    let header0 = txn.get_block_header(BlockNumber(0)).expect("Failed to get header");
    assert!(header0.is_some(), "Header 0 should still be readable");

    let header1 = txn.get_block_header(BlockNumber(1)).expect("Failed to get header");
    assert!(header1.is_some(), "Header 1 should be readable");
}

/// Test multiple read-modify-write cycles within the same batch.
///
/// This test verifies that multiple read-modify-write cycles work correctly
/// within a single batch, with each cycle able to read data from previous cycles.
///
/// # Test Flow
/// 1. Creates storage with batching enabled
/// 2. Performs 5 read-modify-write cycles
/// 3. In each cycle, reads all previously written headers
/// 4. Writes a new header in each cycle
/// 5. Final verification that all 5 headers are readable
#[test]
fn test_multiple_read_modify_write_cycles() {
    let ((_reader, mut writer), _temp_dir) =
        create_storage_with_batch_config(BatchConfig { enabled: true, batch_size: 10 });

    // Perform 5 read-modify-write cycles.
    for i in 0..5 {
        let mut txn = writer.begin_rw_txn().expect("Failed to begin transaction");

        // Read all previously written headers.
        for j in 0..i {
            let header = txn.get_block_header(BlockNumber(j)).expect("Failed to get header");
            assert!(header.is_some(), "Should be able to read header {} in iteration {}", j, i);
        }

        // Write a new header.
        txn = txn
            .append_header(BlockNumber(i), &create_test_header_for_batching(BlockNumber(i)))
            .expect("Failed to append header");

        txn.commit().expect("Failed to commit");
    }

    // Final verification: read all 5 headers.
    let txn = writer.begin_rw_txn().expect("Failed to begin transaction");
    for i in 0..5 {
        let header = txn.get_block_header(BlockNumber(i)).expect("Failed to get header");
        assert!(header.is_some(), "Header {} should be readable at the end", i);
    }
}

/// Verify that uncommitted writes are not visible to the reader (different transaction).
///
/// This test verifies that uncommitted writes in the writer's persistent transaction
/// are not visible to readers using separate read-only transactions.
///
/// # Test Flow
/// 1. Creates storage with batching enabled
/// 2. Writer writes a header (uncommitted, in persistent transaction)
/// 3. Writer can read its own write (same persistent transaction)
/// 4. Reader cannot see the write (different transaction, uncommitted)
#[test]
fn test_read_uncommitted_not_visible_to_reader() {
    let ((reader, mut writer), _temp_dir) =
        create_storage_with_batch_config(BatchConfig { enabled: true, batch_size: 10 });

    // Write header (uncommitted, in persistent txn).
    writer
        .begin_rw_txn()
        .expect("Failed to begin transaction")
        .append_header(BlockNumber(0), &create_test_header_for_batching(BlockNumber(0)))
        .expect("Failed to append header")
        .commit()
        .expect("Failed to commit");

    // Writer can read it (same persistent txn).
    let header_from_writer = writer
        .begin_rw_txn()
        .expect("Failed to begin transaction")
        .get_block_header(BlockNumber(0))
        .expect("Failed to get header");
    assert!(
        header_from_writer.is_some(),
        "Writer should be able to read its own uncommitted write"
    );

    // Reader cannot read it (different transaction, uncommitted).
    let header_from_reader = reader
        .begin_ro_txn()
        .expect("Failed to begin read transaction")
        .get_block_header(BlockNumber(0))
        .expect("Failed to get header");
    assert!(
        header_from_reader.is_none(),
        "Reader should NOT be able to read uncommitted write from writer"
    );
}

/// Verify that after a batch commits, the data is visible to the reader.
///
/// This test verifies that once a batch is flushed and committed to the database,
/// all data in that batch becomes visible to readers.
///
/// # Test Flow
/// 1. Creates storage with batch_size = 3
/// 2. Writes 3 headers to trigger a batch commit
/// 3. Verifies all headers are visible to the reader after batch commit
#[test]
fn test_read_after_batch_commit_visible_to_reader() {
    let batch_size = 3;
    let ((reader, mut writer), _temp_dir) =
        create_storage_with_batch_config(BatchConfig { enabled: true, batch_size });

    // Write batch_size headers to trigger a commit.
    for i in 0..batch_size {
        writer
            .begin_rw_txn()
            .expect("Failed to begin transaction")
            .append_header(
                BlockNumber(i as u64),
                &create_test_header_for_batching(BlockNumber(i as u64)),
            )
            .expect("Failed to append header")
            .commit()
            .expect("Failed to commit");
    }

    // After batch commit, reader should see the data.
    for i in 0..batch_size {
        let header = reader
            .begin_ro_txn()
            .expect("Failed to begin read transaction")
            .get_block_header(BlockNumber(i as u64))
            .expect("Failed to get header");
        assert!(header.is_some(), "Reader should see header {} after batch commit", i);
    }
}

/// Test that data persists to disk after batch commit and storage restart.
///
/// This test verifies that when a batch commits, the data is actually written
/// to the database file on disk, not just visible in memory. It does this by
/// closing and reopening the storage.
///
/// # Test Flow
/// 1. Creates storage with batching enabled (batch_size = 3)
/// 2. Writes 3 headers to complete a batch
/// 3. Drops the storage (closes all file handles)
/// 4. Reopens the storage from the same directory
/// 5. Verifies all headers are still readable (proving disk persistence)
#[test]
fn test_batching_data_persists_after_storage_restart() {
    let batch_size = 3;
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let mut config =
        get_test_config_with_path(Some(StorageScope::FullArchive), temp_dir.path().to_path_buf());

    // Phase 1: Initialize storage without batching (for version setup).
    config.batch_config = BatchConfig { enabled: false, batch_size: 100 };
    {
        let (_reader, _writer) =
            open_storage(config.clone()).expect("Failed to open storage for initialization");
        // Drop to close storage.
    }

    // Phase 2: Write data with batching enabled.
    config.batch_config = BatchConfig { enabled: true, batch_size };
    {
        let (_reader, mut writer) =
            open_storage(config.clone()).expect("Failed to open storage with batching");

        // Reset counter to account for empty commit from set_version_if_needed().
        writer.reset_batch_counter_for_testing();

        // Write batch_size headers to trigger a commit.
        for i in 0..batch_size {
            writer
                .begin_rw_txn()
                .expect("Failed to begin transaction")
                .append_header(
                    BlockNumber(i as u64),
                    &create_test_header_for_batching(BlockNumber(i as u64)),
                )
                .expect("Failed to append header")
                .commit()
                .expect("Failed to commit");
        }
        // Drop storage (closes all file handles, flushes to disk).
    }

    // Phase 3: Reopen storage and verify data persisted to disk.
    config.batch_config = BatchConfig { enabled: false, batch_size: 100 };
    {
        let (reader, _writer) =
            open_storage(config).expect("Failed to reopen storage after restart");

        for i in 0..batch_size {
            let header = reader
                .begin_ro_txn()
                .expect("Failed to begin read transaction")
                .get_block_header(BlockNumber(i as u64))
                .expect("Failed to get header");
            assert!(header.is_some(), "Header {} should persist to disk after storage restart", i);
            assert_eq!(
                header.unwrap().block_header_without_hash.block_number,
                BlockNumber(i as u64),
                "Header {} data should match after restart",
                i
            );
        }
    }
}

/// Test that incomplete batches do not persist after storage restart.
///
/// This test verifies that uncommitted batches (writes that haven't reached
/// batch_size) are not persisted to disk. This ensures proper transaction
/// semantics - only committed batches should survive a restart.
///
/// # Test Flow
/// 1. Creates storage with batching enabled (batch_size = 5)
/// 2. Writes only 3 headers (incomplete batch)
/// 3. Drops the storage without completing the batch
/// 4. Reopens the storage
/// 5. Verifies the headers are NOT present (incomplete batch was rolled back)
#[test]
fn test_batching_incomplete_batch_not_persisted() {
    let batch_size = 5;
    let incomplete_writes = 3;
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let mut config =
        get_test_config_with_path(Some(StorageScope::FullArchive), temp_dir.path().to_path_buf());

    // Phase 1: Initialize storage without batching.
    config.batch_config = BatchConfig { enabled: false, batch_size: 100 };
    {
        let (_reader, _writer) =
            open_storage(config.clone()).expect("Failed to open storage for initialization");
    }

    // Phase 2: Write incomplete batch with batching enabled.
    config.batch_config = BatchConfig { enabled: true, batch_size };
    {
        let (_reader, mut writer) =
            open_storage(config.clone()).expect("Failed to open storage with batching");

        // Write only incomplete_writes headers (less than batch_size).
        for i in 0..incomplete_writes {
            writer
                .begin_rw_txn()
                .expect("Failed to begin transaction")
                .append_header(
                    BlockNumber(i as u64),
                    &create_test_header_for_batching(BlockNumber(i as u64)),
                )
                .expect("Failed to append header")
                .commit()
                .expect("Failed to commit");
        }
        // Drop storage without completing the batch.
    }

    // Phase 3: Reopen and verify incomplete batch was not persisted.
    config.batch_config = BatchConfig { enabled: false, batch_size: 100 };
    {
        let (reader, _writer) =
            open_storage(config).expect("Failed to reopen storage after restart");

        for i in 0..incomplete_writes {
            let header = reader
                .begin_ro_txn()
                .expect("Failed to begin read transaction")
                .get_block_header(BlockNumber(i as u64))
                .expect("Failed to get header");
            assert!(
                header.is_none(),
                "Header {} from incomplete batch should NOT persist after restart",
                i
            );
        }
    }
}

/// Test that multiple batches persist correctly after storage restart.
///
/// This test verifies that when multiple batches are committed, all of them
/// are properly persisted to disk and survive a storage restart.
///
/// # Test Flow
/// 1. Creates storage with batching enabled (batch_size = 3)
/// 2. Writes 2 complete batches (6 headers total)
/// 3. Drops the storage
/// 4. Reopens the storage
/// 5. Verifies all 6 headers are present (both batches persisted)
#[test]
fn test_batching_multiple_batches_persist() {
    let batch_size = 3;
    let num_batches = 2;
    let total_headers = batch_size * num_batches;
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let mut config =
        get_test_config_with_path(Some(StorageScope::FullArchive), temp_dir.path().to_path_buf());

    // Phase 1: Initialize storage.
    config.batch_config = BatchConfig { enabled: false, batch_size: 100 };
    {
        let (_reader, _writer) =
            open_storage(config.clone()).expect("Failed to open storage for initialization");
    }

    // Phase 2: Write multiple complete batches.
    config.batch_config = BatchConfig { enabled: true, batch_size };
    {
        let (_reader, mut writer) =
            open_storage(config.clone()).expect("Failed to open storage with batching");

        // Reset counter to account for empty commit from set_version_if_needed().
        writer.reset_batch_counter_for_testing();

        for i in 0..total_headers {
            writer
                .begin_rw_txn()
                .expect("Failed to begin transaction")
                .append_header(
                    BlockNumber(i as u64),
                    &create_test_header_for_batching(BlockNumber(i as u64)),
                )
                .expect("Failed to append header")
                .commit()
                .expect("Failed to commit");
        }
        // Drop storage after writing multiple complete batches.
    }

    // Phase 3: Reopen and verify all batches persisted.
    config.batch_config = BatchConfig { enabled: false, batch_size: 100 };
    {
        let (reader, _writer) =
            open_storage(config).expect("Failed to reopen storage after restart");

        for i in 0..total_headers {
            let header = reader
                .begin_ro_txn()
                .expect("Failed to begin read transaction")
                .get_block_header(BlockNumber(i as u64))
                .expect("Failed to get header");
            assert!(
                header.is_some(),
                "Header {} from batch {} should persist after restart",
                i,
                i / batch_size
            );
        }
    }
}
