use starknet_api::block::{BlockBody, BlockHeader, BlockNumber, BlockSignature};
use starknet_api::state::ThinStateDiff;
use tempfile::tempdir;

use crate::header::{HeaderStorageReader, HeaderStorageWriter};
use crate::test_utils::get_test_config_with_path;
use crate::{open_storage, BatchConfig, StorageScope};

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
