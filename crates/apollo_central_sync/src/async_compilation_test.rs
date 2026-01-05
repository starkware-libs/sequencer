use std::sync::Arc;

use apollo_storage::header::HeaderStorageReader;
use apollo_storage::test_utils::get_test_storage;
use apollo_test_utils::{GetTestInstance, get_rng};
use starknet_api::block::{BlockHash, BlockHeader, BlockNumber};
use starknet_api::felt;
use tokio::sync::Mutex;

/// Test that storage batching works correctly with the queue API.
#[tokio::test]
async fn test_storage_batching_with_queue() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();
    let mut rng = get_rng();

    // Queue multiple writes- start from BlockNumber(0) for sequential writes.
    let block_0 = BlockNumber(0);
    let block_1 = BlockNumber(1);
    let block_2 = BlockNumber(2);

    let mut header_0 = BlockHeader::get_test_instance(&mut rng);
    let mut header_1 = BlockHeader::get_test_instance(&mut rng);
    let mut header_2 = BlockHeader::get_test_instance(&mut rng);

    // Ensure unique block hashes.
    header_0.block_hash = BlockHash(felt!("0x100"));
    header_1.block_hash = BlockHash(felt!("0x101"));
    header_2.block_hash = BlockHash(felt!("0x102"));

    // Queue headers.
    writer.queue_header(block_0, header_0.clone()).unwrap();
    writer.queue_header(block_1, header_1.clone()).unwrap();
    writer.queue_header(block_2, header_2.clone()).unwrap();

    // Flush the batch.
    writer.flush_batch().unwrap();

    // Verify all headers were written.
    let txn = reader.begin_ro_txn().unwrap();
    assert_eq!(txn.get_block_header(block_0).unwrap(), Some(header_0));
    assert_eq!(txn.get_block_header(block_1).unwrap(), Some(header_1));
    assert_eq!(txn.get_block_header(block_2).unwrap(), Some(header_2));
}

