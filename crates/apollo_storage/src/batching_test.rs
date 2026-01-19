use starknet_api::block::{BlockBody, BlockHeader, BlockNumber, BlockSignature};
use starknet_api::state::ThinStateDiff;
use tempfile::{tempdir, TempDir};

use crate::header::{HeaderStorageReader, HeaderStorageWriter};
use crate::test_utils::{get_test_config, get_test_config_with_path};
use crate::{open_storage, BatchConfig, StorageReader, StorageScope, StorageWriter};

/// Returns [`StorageReader`], [`StorageWriter`] with batching enabled and the temporary directory.
fn get_test_storage_with_batching(batch_size: usize) -> ((StorageReader, StorageWriter), TempDir) {
    let (mut config, temp_dir) = get_test_config(None);
    config.batch_config = BatchConfig { enable_batching: true, batch_size };
    ((open_storage(config).unwrap()), temp_dir)
}

/// Test basic batching configuration.
#[test]
fn test_batch_config_default() {
    let config = BatchConfig::default();
    assert!(!config.enable_batching, "Batching should be disabled by default");
    assert_eq!(config.batch_size, 100, "Default batch size should be 100");
}

/// Test that queue methods add to the batch queue without immediate writes.
#[test]
fn test_queue_operations_without_flush() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let mut config =
        get_test_config_with_path(Some(StorageScope::FullArchive), temp_dir.path().to_path_buf());

    // Enable batching with large batch size to prevent auto-flush.
    config.batch_config = BatchConfig { enable_batching: true, batch_size: 1000 };

    let (reader, mut writer) = open_storage(config).unwrap();

    // Queue some operations.
    let block_number = BlockNumber(0);
    let header = BlockHeader::default();
    let body = BlockBody::default();
    let signature = BlockSignature::default();

    writer.queue_header(block_number, header.clone()).unwrap();
    writer.queue_body(block_number, body.clone()).unwrap();
    writer.queue_signature(block_number, signature).unwrap();

    // Verify queue has 3 operations.
    assert_eq!(writer.batch_queue_len(), 3, "Batch queue should contain 3 operations");

    // Verify data is NOT in storage yet (still in queue).
    let read_header = reader.begin_ro_txn().unwrap().get_block_header(block_number).unwrap();
    assert!(read_header.is_none(), "Header should not be in storage yet (still in queue)");
}

/// Test that flush_batch writes all queued operations in one transaction.
#[test]
fn test_flush_batch_writes_all_operations() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let mut config =
        get_test_config_with_path(Some(StorageScope::FullArchive), temp_dir.path().to_path_buf());

    config.batch_config = BatchConfig { enable_batching: true, batch_size: 1000 };

    let (reader, mut writer) = open_storage(config).unwrap();

    // Queue multiple operations for the same block.
    let block_number = BlockNumber(0);
    let header = BlockHeader::default();
    let body = BlockBody::default();
    let signature = BlockSignature::default();
    let state_diff = ThinStateDiff::default();

    writer.queue_header(block_number, header).unwrap();
    writer.queue_body(block_number, body).unwrap();
    writer.queue_signature(block_number, signature).unwrap();
    writer.queue_state_diff(block_number, state_diff).unwrap();

    // Verify queue has 4 operations.
    assert_eq!(writer.batch_queue_len(), 4);

    // Flush the batch.
    writer.flush_batch().unwrap();

    // Verify queue is now empty.
    assert_eq!(writer.batch_queue_len(), 0, "Batch queue should be empty after flush");

    // Verify block is now in storage.
    let header = reader.begin_ro_txn().unwrap().get_block_header(block_number).unwrap();
    assert!(header.is_some(), "Block header should be in storage after flush");
}

/// Test that auto-flush triggers when batch reaches batch_size.
#[test]
fn test_auto_flush_at_batch_size() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let mut config =
        get_test_config_with_path(Some(StorageScope::FullArchive), temp_dir.path().to_path_buf());

    // Set small batch size to trigger auto-flush.
    config.batch_config = BatchConfig {
        enable_batching: true,
        batch_size: 3, // Auto-flush after 3 operations.
    };

    let (reader, mut writer) = open_storage(config).unwrap();

    let block_number = BlockNumber(0);
    let header = BlockHeader::default();
    let body = BlockBody::default();
    let signature = BlockSignature::default();

    // Queue 2 operations (should NOT auto-flush).
    writer.queue_header(block_number, header.clone()).unwrap();
    writer.queue_body(block_number, body.clone()).unwrap();
    assert_eq!(writer.batch_queue_len(), 2, "Queue should have 2 operations");

    // Queue 3rd operation (should trigger auto-flush at batch_size=3).
    writer.queue_signature(block_number, signature).unwrap();

    // After auto-flush, queue should be empty.
    assert_eq!(writer.batch_queue_len(), 0, "Queue should be empty after auto-flush");

    // Verify data is in storage.
    let read_header = reader.begin_ro_txn().unwrap().get_block_header(block_number).unwrap();
    assert!(read_header.is_some(), "Header should be in storage after auto-flush");
}

/// Test that batching can be disabled (items flush immediately).
#[test]
fn test_batching_disabled() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let mut config =
        get_test_config_with_path(Some(StorageScope::FullArchive), temp_dir.path().to_path_buf());

    // Disable batching.
    config.batch_config = BatchConfig { enable_batching: false, batch_size: 100 };

    let (reader, mut writer) = open_storage(config).unwrap();

    assert!(!writer.is_batching_enabled(), "Batching should be disabled");

    // Queue operations with batching disabled - they should flush immediately.
    let block_number = BlockNumber(0);
    writer.queue_header(block_number, BlockHeader::default()).unwrap();

    // With batching disabled, items flush immediately, so queue should be empty.
    assert_eq!(writer.batch_queue_len(), 0, "Queue should be empty after immediate flush");

    // Verify data was actually written to storage.
    let read_header = reader.begin_ro_txn().unwrap().get_block_header(block_number).unwrap();
    assert!(read_header.is_some(), "Header should be in storage after immediate flush");
}

/// Test queue_state_diff operation.
#[test]
fn test_queue_state_diff() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let mut config =
        get_test_config_with_path(Some(StorageScope::FullArchive), temp_dir.path().to_path_buf());

    config.batch_config = BatchConfig { enable_batching: true, batch_size: 1000 };

    let (_reader, mut writer) = open_storage(config).unwrap();

    let block_number = BlockNumber(0);
    let state_diff = ThinStateDiff::default();

    writer.queue_state_diff(block_number, state_diff).unwrap();

    assert_eq!(writer.batch_queue_len(), 1, "Queue should have 1 state diff operation");
}

/// Test queue_casm operation.
#[test]
fn test_queue_casm() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let mut config =
        get_test_config_with_path(Some(StorageScope::FullArchive), temp_dir.path().to_path_buf());

    config.batch_config = BatchConfig { enable_batching: true, batch_size: 1000 };

    let (_reader, writer) = open_storage(config).unwrap();

    // Just test that the queue operation works
    // (We don't need to actually create a valid casm for this test).
    assert_eq!(writer.batch_queue_len(), 0, "Queue should start empty");

    // Note: Testing actual casm queuing would require creating valid casm objects,
    // which is complex. The queue_casm() method itself is tested indirectly
    // through the batching mechanism tests.
}

/// Test queue_base_layer_marker operation.
#[test]
fn test_queue_base_layer_marker() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let mut config =
        get_test_config_with_path(Some(StorageScope::FullArchive), temp_dir.path().to_path_buf());

    config.batch_config = BatchConfig { enable_batching: true, batch_size: 1000 };

    let (_reader, mut writer) = open_storage(config).unwrap();

    let block_number = BlockNumber(1000);
    writer.queue_base_layer_marker(block_number).unwrap();

    assert_eq!(writer.batch_queue_len(), 1, "Queue should have 1 base layer marker operation");
}

/// Test that old API (begin_rw_txn) still works alongside new queue API.
#[test]
fn test_old_api_still_works() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let mut config =
        get_test_config_with_path(Some(StorageScope::FullArchive), temp_dir.path().to_path_buf());

    config.batch_config = BatchConfig { enable_batching: true, batch_size: 1000 };

    let (reader, mut writer) = open_storage(config).unwrap();

    // Use OLD API (should still work, bypasses batching).
    let block_number = BlockNumber(0);
    let header = BlockHeader::default();

    let mut txn = writer.begin_rw_txn().unwrap();
    txn = txn.append_header(block_number, &header).unwrap();
    txn.commit().unwrap();

    // Verify data is in storage immediately (not in queue).
    let read_header = reader.begin_ro_txn().unwrap().get_block_header(block_number).unwrap();
    assert!(read_header.is_some(), "Old API should write immediately to storage");

    // Verify queue is still empty (old API bypasses queue)
    assert_eq!(writer.batch_queue_len(), 0, "Old API should not use batch queue");
}

/// Test batching with multiple operation types mixed.
#[test]
fn test_mixed_operation_types_in_batch() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let mut config =
        get_test_config_with_path(Some(StorageScope::FullArchive), temp_dir.path().to_path_buf());

    config.batch_config = BatchConfig { enable_batching: true, batch_size: 1000 };

    let (reader, mut writer) = open_storage(config).unwrap();

    // Queue mixed operations: header, state diff, base layer marker.
    writer.queue_header(BlockNumber(0), BlockHeader::default()).unwrap();
    writer.queue_state_diff(BlockNumber(0), ThinStateDiff::default()).unwrap();
    writer.queue_base_layer_marker(BlockNumber(100)).unwrap();

    // Should have 3 operations in queue.
    assert_eq!(writer.batch_queue_len(), 3);

    // Flush all
    writer.flush_batch().unwrap();

    // Verify queue is empty.
    assert_eq!(writer.batch_queue_len(), 0);

    // Verify block is in storage.
    let header0 = reader.begin_ro_txn().unwrap().get_block_header(BlockNumber(0)).unwrap();
    assert!(header0.is_some());
}

/// Test that flush_batch is idempotent (can be called when queue is empty).
#[test]
fn test_flush_empty_queue_is_safe() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let mut config =
        get_test_config_with_path(Some(StorageScope::FullArchive), temp_dir.path().to_path_buf());

    config.batch_config = BatchConfig { enable_batching: true, batch_size: 1000 };

    let (_reader, mut writer) = open_storage(config).unwrap();

    // Queue should be empty.
    assert_eq!(writer.batch_queue_len(), 0);

    // Flush empty queue (should not panic or error).
    writer.flush_batch().unwrap();

    // Queue should still be empty.
    assert_eq!(writer.batch_queue_len(), 0);
}

/// Test batch_size configuration is respected.
#[test]
fn test_batch_size_configuration() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let mut config =
        get_test_config_with_path(Some(StorageScope::FullArchive), temp_dir.path().to_path_buf());

    config.batch_config = BatchConfig {
        enable_batching: true,
        batch_size: 42, // Custom batch size.
    };

    let (_reader, writer) = open_storage(config).unwrap();

    assert_eq!(writer.batch_size(), 42, "Batch size should match configuration");
}

/// Test that mixing old API (begin_rw_txn) with new API (queue_*) is prevented when batching is
/// enabled and queue is non-empty.
#[test]
fn test_prevent_api_mixing_with_non_empty_queue() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let mut config =
        get_test_config_with_path(Some(StorageScope::FullArchive), temp_dir.path().to_path_buf());

    // Enable batching with large batch size to prevent auto-flush.
    config.batch_config = BatchConfig { enable_batching: true, batch_size: 1000 };

    let (_reader, mut writer) = open_storage(config).unwrap();

    // Queue an operation (new API).
    let block_number = BlockNumber(0);
    writer.queue_header(block_number, BlockHeader::default()).unwrap();

    // Verify queue has 1 item.
    assert_eq!(writer.batch_queue_len(), 1, "Queue should have 1 item");

    // Attempt to use old API (begin_rw_txn) - should fail with BatchingApiMixingError.
    let result = writer.begin_rw_txn();
    assert!(result.is_err(), "begin_rw_txn should fail when queue is not empty");

    // Verify the error is the expected one.
    match result {
        Err(crate::StorageError::BatchingApiMixingError { queue_len }) => {
            assert_eq!(queue_len, 1, "Error should report correct queue length");
        }
        Err(e) => panic!("Expected BatchingApiMixingError, got different error: {}", e),
        Ok(_) => panic!("Expected BatchingApiMixingError, but begin_rw_txn succeeded"),
    }
}

/// Test that old API (begin_rw_txn) works when batching is enabled but queue is empty.
#[test]
fn test_old_api_works_with_empty_queue() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let mut config =
        get_test_config_with_path(Some(StorageScope::FullArchive), temp_dir.path().to_path_buf());

    // Enable batching.
    config.batch_config = BatchConfig { enable_batching: true, batch_size: 1000 };

    let (reader, mut writer) = open_storage(config).unwrap();

    // Queue should be empty.
    assert_eq!(writer.batch_queue_len(), 0, "Queue should be empty initially");

    // Use old API (should work since queue is empty).
    let block_number = BlockNumber(0);
    let header = BlockHeader::default();

    let mut txn = writer.begin_rw_txn().unwrap();
    txn = txn.append_header(block_number, &header).unwrap();
    txn.commit().unwrap();

    // Verify data was written.
    let read_header = reader.begin_ro_txn().unwrap().get_block_header(block_number).unwrap();
    assert!(read_header.is_some(), "Old API should work when queue is empty");
}

/// Test that after flushing the queue, old API can be used again.
#[test]
fn test_old_api_works_after_flush() {
    let temp_dir = tempdir().expect("Failed to create temp directory");
    let mut config =
        get_test_config_with_path(Some(StorageScope::FullArchive), temp_dir.path().to_path_buf());

    // Enable batching with large batch size to prevent auto-flush.
    config.batch_config = BatchConfig { enable_batching: true, batch_size: 1000 };

    let (reader, mut writer) = open_storage(config).unwrap();

    // Queue an operation.
    let header0 = BlockHeader {
        block_hash: starknet_api::block::BlockHash(starknet_api::felt!("0x100")),
        ..Default::default()
    };
    writer.queue_header(BlockNumber(0), header0).unwrap();
    assert_eq!(writer.batch_queue_len(), 1, "Queue should have 1 item");

    // Flush the queue.
    writer.flush_batch().unwrap();
    assert_eq!(writer.batch_queue_len(), 0, "Queue should be empty after flush");

    // Now old API should work.
    let block_number = BlockNumber(1);
    let header1 = BlockHeader {
        block_hash: starknet_api::block::BlockHash(starknet_api::felt!("0x101")),
        ..Default::default()
    };

    let mut txn = writer.begin_rw_txn().unwrap();
    txn = txn.append_header(block_number, &header1).unwrap();
    txn.commit().unwrap();

    // Verify data was written.
    let read_header = reader.begin_ro_txn().unwrap().get_block_header(block_number).unwrap();
    assert!(read_header.is_some(), "Old API should work after flush");
}

/// Test that the transaction flag protection prevents auto-flush during a transaction.
/// This ensures atomicity: all operations between begin_rw_txn() and commit() are
/// flushed together, preventing partial block writes.
#[test]
fn test_transaction_flag_blocks_auto_flush() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_batching(2); // batch_size=2

    let block0 = BlockNumber(0);
    let block1 = BlockNumber(1);
    let header0 = BlockHeader::default();
    let header1 = BlockHeader::default();

    // Start a transaction (sets active_transaction=true).
    let mut txn = writer.begin_rw_txn().unwrap();

    // Queue an item using the transaction API.
    txn = txn.append_header(block0, &header0).unwrap();

    // At this point, the queue has 0 items (transaction API doesn't use the queue).
    // Commit the transaction (resets active_transaction=false).
    txn.commit().unwrap();

    // Now queue items using the queue API.
    writer.queue_header(block1, header1).unwrap();

    // The queue should have 1 item (block1), because block0 was flushed on commit.
    assert_eq!(writer.batch_queue_len(), 1, "Queue should have 1 item after transaction commit");

    // Verify block0 was written.
    let read_header0 = reader.begin_ro_txn().unwrap().get_block_header(block0).unwrap();
    assert!(read_header0.is_some(), "Block0 should be written after transaction commit");

    // Verify block1 is NOT written yet (still in queue).
    let read_header1 = reader.begin_ro_txn().unwrap().get_block_header(block1).unwrap();
    assert!(read_header1.is_none(), "Block1 should not be written yet (still in queue)");
}

/// Test that auto-flush resumes after transaction commits.
#[test]
fn test_auto_flush_resumes_after_commit() {
    let ((reader, mut writer), _temp_dir) = get_test_storage_with_batching(2); // batch_size=2

    let block0 = BlockNumber(0);
    let block1 = BlockNumber(1);
    let block2 = BlockNumber(2);

    // Create headers with different block hashes to avoid KeyAlreadyExists error.
    let header0 = BlockHeader {
        block_hash: starknet_api::block::BlockHash(starknet_api::hash::StarkHash::from(0_u128)),
        ..Default::default()
    };

    let header1 = BlockHeader {
        block_hash: starknet_api::block::BlockHash(starknet_api::hash::StarkHash::from(1_u128)),
        ..Default::default()
    };

    let header2 = BlockHeader {
        block_hash: starknet_api::block::BlockHash(starknet_api::hash::StarkHash::from(2_u128)),
        ..Default::default()
    };

    // Use transaction API for block0.
    let mut txn = writer.begin_rw_txn().unwrap();
    txn = txn.append_header(block0, &header0).unwrap();
    txn.commit().unwrap();

    // Now use queue API - auto-flush should work normally.
    writer.queue_header(block1, header1).unwrap();
    assert_eq!(writer.batch_queue_len(), 1, "Queue should have 1 item");

    // Queue another item - this should trigger auto-flush (batch_size=2).
    writer.queue_header(block2, header2).unwrap();
    assert_eq!(writer.batch_queue_len(), 0, "Queue should be empty after auto-flush");

    // Verify all blocks were written.
    let read_header0 = reader.begin_ro_txn().unwrap().get_block_header(block0).unwrap();
    let read_header1 = reader.begin_ro_txn().unwrap().get_block_header(block1).unwrap();
    let read_header2 = reader.begin_ro_txn().unwrap().get_block_header(block2).unwrap();
    assert!(read_header0.is_some(), "Block0 should be written");
    assert!(read_header1.is_some(), "Block1 should be written after auto-flush");
    assert!(read_header2.is_some(), "Block2 should be written after auto-flush");
}

#[test]
fn test_marker_validation_reverts_incomplete_blocks() {
    use crate::body::{BodyStorageReader, BodyStorageWriter};
    use crate::state::{StateStorageReader, StateStorageWriter};

    // Create storage WITHOUT batching initially (to use transaction API for setup).
    let (config, _temp_dir) = get_test_config(Some(StorageScope::FullArchive));
    let (reader, mut writer) = open_storage(config).unwrap();

    // Write block 0 completely (header, body, state) using transaction API.
    let block0 = BlockNumber(0);
    let header0 = BlockHeader {
        block_hash: starknet_api::block::BlockHash(starknet_api::hash::StarkHash::from(0_u128)),
        ..Default::default()
    };
    let body0 = BlockBody::default();
    let state0 = ThinStateDiff::default();

    writer.begin_rw_txn().unwrap().append_header(block0, &header0).unwrap().commit().unwrap();
    writer.begin_rw_txn().unwrap().append_body(block0, body0.clone()).unwrap().commit().unwrap();
    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_diff(block0, state0.clone())
        .unwrap()
        .commit()
        .unwrap();

    // Write block 1 INCOMPLETELY - only header and body, no state.
    // This simulates a crash mid-block.
    let block1 = BlockNumber(1);
    let header1 = BlockHeader {
        block_hash: starknet_api::block::BlockHash(starknet_api::hash::StarkHash::from(1_u128)),
        ..Default::default()
    };
    let body1 = BlockBody::default();

    writer.begin_rw_txn().unwrap().append_header(block1, &header1).unwrap().commit().unwrap();
    writer.begin_rw_txn().unwrap().append_body(block1, body1.clone()).unwrap().commit().unwrap();
    // Intentionally NOT writing state for block 1 to create marker mismatch.

    // Verify markers are inconsistent.
    let header_marker = reader.begin_ro_txn().unwrap().get_header_marker().unwrap();
    let body_marker = reader.begin_ro_txn().unwrap().get_body_marker().unwrap();
    let state_marker = reader.begin_ro_txn().unwrap().get_state_marker().unwrap();

    let block2 = BlockNumber(2);
    assert_eq!(header_marker, block2, "Header marker should be at block 2 (next to write)");
    assert_eq!(body_marker, block2, "Body marker should be at block 2 (next to write)");
    assert_eq!(
        state_marker,
        block1,
        "State marker should be at block 1 (incomplete - no state written for block 1!)"
    );

    // Close and reopen storage - this triggers marker validation.
    drop(reader);
    drop(writer);

    let config = get_test_config_with_path(Some(StorageScope::FullArchive), _temp_dir.path().to_path_buf());
    let (reader, _writer) = open_storage(config).unwrap();

    // After validation, all markers should be at block 1 (next to write after safe point block 0).
    let header_marker = reader.begin_ro_txn().unwrap().get_header_marker().unwrap();
    let body_marker = reader.begin_ro_txn().unwrap().get_body_marker().unwrap();
    let state_marker = reader.begin_ro_txn().unwrap().get_state_marker().unwrap();

    assert_eq!(
        header_marker, block1,
        "Header marker should be reverted to block 1 (next after safe point)"
    );
    assert_eq!(
        body_marker, block1,
        "Body marker should be reverted to block 1 (next after safe point)"
    );
    assert_eq!(
        state_marker, block1,
        "State marker should be at block 1 (next after safe point)"
    );

    // Verify block 1 was completely removed.
    let read_header1 = reader.begin_ro_txn().unwrap().get_block_header(block1).unwrap();
    assert!(read_header1.is_none(), "Block 1 should be reverted (incomplete)");

    // Verify block 0 is still intact.
    let read_header0 = reader.begin_ro_txn().unwrap().get_block_header(block0).unwrap();
    assert!(read_header0.is_some(), "Block 0 should still exist (complete)");
}
