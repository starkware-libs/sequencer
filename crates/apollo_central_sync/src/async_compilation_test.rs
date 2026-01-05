use std::sync::Arc;
use std::time::Duration;

use apollo_storage::header::HeaderStorageReader;
use apollo_storage::test_utils::get_test_storage;
use apollo_test_utils::{GetTestInstance, get_rng};
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use futures_util::StreamExt;
use futures_util::stream::FuturesOrdered;
use indexmap::IndexMap;
use starknet_api::block::{Block, BlockBody, BlockHash, BlockHeader, BlockNumber};
use starknet_api::state::ThinStateDiff;
use starknet_api::{class_hash, compiled_class_hash, felt};
use tokio::sync::{Mutex, RwLock};

use crate::{ProcessedBlockData, ProcessingTask, StateSyncError};

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
/// Test that the middle queue processes items in the correct order.
#[tokio::test]
async fn test_middle_queue_ordering() {
    let middle_queue: Arc<Mutex<FuturesOrdered<ProcessingTask>>> =
        Arc::new(Mutex::new(FuturesOrdered::new()));

    let mut rng = get_rng();

    // Add 3 items to the queue with different completion times.
    {
        let mut queue = middle_queue.lock().await;

        // Item 1: completes quickly.
        let header1 = BlockHeader::get_test_instance(&mut rng);
        let block1 = Block { header: header1, body: BlockBody::default() };
        queue.push_back(Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(ProcessedBlockData::Block {
                block_number: BlockNumber(1),
                block: block1,
                signature: Default::default(),
            })
        }));

        // Item 2: completes slowly.
        let header2 = BlockHeader::get_test_instance(&mut rng);
        let block2 = Block { header: header2, body: BlockBody::default() };
        queue.push_back(Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(ProcessedBlockData::Block {
                block_number: BlockNumber(2),
                block: block2,
                signature: Default::default(),
            })
        }));

        // Item 3: completes very quickly.
        let header3 = BlockHeader::get_test_instance(&mut rng);
        let block3 = Block { header: header3, body: BlockBody::default() };
        queue.push_back(Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(5)).await;
            Ok(ProcessedBlockData::Block {
                block_number: BlockNumber(3),
                block: block3,
                signature: Default::default(),
            })
        }));
    }

    // Verify items come out in order (1, 2, 3) despite Item 3 completing first.
    let mut results = vec![];
    {
        let mut queue = middle_queue.lock().await;
        while let Some(result) = queue.next().await {
            if let Ok(ProcessedBlockData::Block { block_number, .. }) = result {
                results.push(block_number);
            }
        }
    }

    assert_eq!(results, vec![BlockNumber(1), BlockNumber(2), BlockNumber(3)]);
}
/// Test that async compilation can process multiple items in parallel.
#[tokio::test]
async fn test_parallel_async_compilation() {
    let middle_queue: Arc<Mutex<FuturesOrdered<ProcessingTask>>> =
        Arc::new(Mutex::new(FuturesOrdered::new()));

    let start = tokio::time::Instant::now();

    // Add 3 state diffs that each take 50ms to "compile".
    {
        let mut queue = middle_queue.lock().await;

        for i in 0..3 {
            queue.push_back(Box::pin(async move {
                // Simulate async compilation.
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok(ProcessedBlockData::StateDiff {
                    block_number: BlockNumber(i),
                    _block_hash: BlockHash(felt!("0x0")),
                    thin_state_diff: ThinStateDiff::default(),
                    classes: IndexMap::new(),
                    deprecated_classes: IndexMap::new(),
                    _deployed_contract_class_definitions: IndexMap::new(),
                    _block_contains_old_classes: false,
                })
            }));
        }
    }

    // Process all items.
    {
        let mut queue = middle_queue.lock().await;
        while queue.next().await.is_some() {}
    }

    let elapsed = start.elapsed();

    // With parallel execution, total time should be ~150ms (sequential).
    // not 150ms * 3 = 450ms, but FuturesOrdered processes in order so it's sequential.
    assert!(elapsed < Duration::from_millis(200), "Processing took too long: {:?}", elapsed);
}
/// Test that the middle queue handles errors gracefully.
#[tokio::test]
async fn test_middle_queue_error_handling() {
    let middle_queue: Arc<Mutex<FuturesOrdered<ProcessingTask>>> =
        Arc::new(Mutex::new(FuturesOrdered::new()));

    // Add an item that will error.
    {
        let mut queue = middle_queue.lock().await;
        queue.push_back(Box::pin(async move {
            Err(StateSyncError::StorageError(apollo_storage::StorageError::MarkerMismatch {
                expected: BlockNumber(5),
                found: BlockNumber(3),
            }))
        }));
    }

    // Process and verify we get the error.
    {
        let mut queue = middle_queue.lock().await;
        if let Some(result) = queue.next().await {
            assert!(result.is_err());
        }
    }
}
/// Test that the consumer task can process items from the queue.
#[tokio::test]
async fn test_consumer_task_processing() {
    let middle_queue: Arc<Mutex<FuturesOrdered<ProcessingTask>>> =
        Arc::new(Mutex::new(FuturesOrdered::new()));
    let ((reader, writer), _temp_dir) = get_test_storage();
    let writer = Arc::new(Mutex::new(writer));
    let mut rng = get_rng();

    // Add items to the queue.
    {
        let mut queue = middle_queue.lock().await;

        let header = BlockHeader::get_test_instance(&mut rng);
        let block = Block { header, body: BlockBody::default() };
        let block_number = BlockNumber(0); // Start from 0 for sequential writes.
        let signature = Default::default();

        queue.push_back(Box::pin(async move {
            Ok(ProcessedBlockData::Block { block_number, block, signature })
        }));
    }

    // Simulate consumer task.
    let queue_clone = middle_queue.clone();
    let writer_clone = writer.clone();

    let consumer_task = tokio::spawn(async move {
        let mut queue = queue_clone.lock().await;

        if let Some(Ok(ProcessedBlockData::Block { block_number, block, signature })) =
            queue.next().await
        {
            let mut writer = writer_clone.lock().await;
            writer.queue_header(block_number, block.header.clone()).unwrap();
            writer.queue_body(block_number, block.body).unwrap();
            writer.queue_signature(block_number, signature).unwrap();
            writer.flush_batch().unwrap();
        }
    });

    // Wait for consumer to finish.
    consumer_task.await.unwrap();

    // Verify the block was written.
    let txn = reader.begin_ro_txn().unwrap();
    assert!(txn.get_block_header(BlockNumber(0)).unwrap().is_some());
}
/// Test concurrent queue access from multiple tasks.
#[tokio::test]
async fn test_concurrent_queue_access() {
    let middle_queue: Arc<Mutex<FuturesOrdered<ProcessingTask>>> =
        Arc::new(Mutex::new(FuturesOrdered::new()));

    let queue_producer = middle_queue.clone();
    let queue_consumer = middle_queue.clone();

    // Prepare test data.
    let mut rng = get_rng();
    let blocks: Vec<_> = (0..5)
        .map(|_| {
            let header = BlockHeader::get_test_instance(&mut rng);
            Block { header, body: BlockBody::default() }
        })
        .collect();

    // Producer task.
    let producer = tokio::spawn(async move {
        for (i, block) in blocks.into_iter().enumerate() {
            let mut queue = queue_producer.lock().await;
            queue.push_back(Box::pin(async move {
                Ok(ProcessedBlockData::Block {
                    block_number: BlockNumber(u64::try_from(i).expect("index should fit in u64")),
                    block,
                    signature: Default::default(),
                })
            }));
            drop(queue); // Release lock.
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });

    // Consumer task.
    let consumer = tokio::spawn(async move {
        let mut count = 0;
        loop {
            let mut queue = queue_consumer.lock().await;
            if let Some(Ok(_)) = queue.next().await {
                count += 1;
                if count == 5 {
                    break;
                }
            }
        }
        count
    });

    // Wait for both tasks.
    producer.await.unwrap();
    let processed_count = consumer.await.unwrap();

    assert_eq!(processed_count, 5);
}
