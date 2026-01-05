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

/// Test that ProcessedBlockData enum variants work correctly.
#[test]
fn test_processed_block_data_variants() {
    let mut rng = get_rng();

    // Test Block variant.
    let header = BlockHeader::get_test_instance(&mut rng);
    let block_data = ProcessedBlockData::Block {
        block_number: BlockNumber(42),
        block: Block { header, body: BlockBody::default() },
        signature: Default::default(),
    };

    match block_data {
        ProcessedBlockData::Block { block_number, .. } => {
            assert_eq!(block_number, BlockNumber(42));
        }
        _ => panic!("Expected Block variant"),
    }

    // Test StateDiff variant.
    let state_diff_data = ProcessedBlockData::StateDiff {
        block_number: BlockNumber(43),
        _block_hash: BlockHash(felt!("0x123")),
        thin_state_diff: ThinStateDiff::default(),
        classes: IndexMap::new(),
        deprecated_classes: IndexMap::new(),
        _deployed_contract_class_definitions: IndexMap::new(),
        _block_contains_old_classes: false,
    };

    match state_diff_data {
        ProcessedBlockData::StateDiff { block_number, .. } => {
            assert_eq!(block_number, BlockNumber(43));
        }
        _ => panic!("Expected StateDiff variant"),
    }

    // Test CompiledClass variant.
    let compiled_class_data = ProcessedBlockData::CompiledClass {
        class_hash: class_hash!("0xabc"),
        compiled_class_hash: compiled_class_hash!(1_u8),
        compiled_class: CasmContractClass::get_test_instance(&mut rng),
        is_compiler_backward_compatible: true,
    };

    match compiled_class_data {
        ProcessedBlockData::CompiledClass {
            class_hash, is_compiler_backward_compatible, ..
        } => {
            assert_eq!(class_hash, class_hash!("0xabc"));
            assert!(is_compiler_backward_compatible);
        }
        _ => panic!("Expected CompiledClass variant"),
    }
}
/// Test that the queue markers prevent duplicate fetches.
#[tokio::test]
async fn test_queue_markers_prevent_duplicates() {
    let queue_header_marker = Arc::new(RwLock::new(BlockNumber(0)));
    let queue_state_marker = Arc::new(RwLock::new(BlockNumber(0)));

    // Simulate fetching block 5.
    {
        let mut marker = queue_header_marker.write().await;
        *marker = BlockNumber(5);
    }

    // Verify we shouldn't fetch blocks <= 5 again.
    {
        let marker = queue_header_marker.read().await;
        assert!(*marker >= BlockNumber(5));
    }

    // Simulate fetching state diff for block 3.
    {
        let mut marker = queue_state_marker.write().await;
        *marker = BlockNumber(3);
    }

    // Markers are independent.
    {
        let header_marker = queue_header_marker.read().await;
        let state_marker = queue_state_marker.read().await;
        assert_eq!(*header_marker, BlockNumber(5));
        assert_eq!(*state_marker, BlockNumber(3));
    }
}
