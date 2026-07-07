use std::num::NonZeroUsize;
use std::pin::Pin;

use apollo_starknet_client::reader::{GenericContractClass, MockStarknetReader};
use apollo_storage::test_utils::get_test_storage;
use futures_util::StreamExt;
use mockall::predicate;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::class_hash;
use starknet_api::core::{ClassHash, GlobalRoot};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;

use super::{StateUpdateStream, StateUpdateStreamConfig};

fn test_config(max_classes_to_store_in_memory: usize) -> StateUpdateStreamConfig {
    StateUpdateStreamConfig {
        max_state_updates_to_download: 100,
        max_state_updates_to_store_in_memory: 100,
        max_classes_to_download: 100,
        max_classes_to_store_in_memory,
    }
}

fn state_update_with_classes(
    class_hashes: Vec<ClassHash>,
) -> apollo_starknet_client::reader::StateUpdate {
    apollo_starknet_client::reader::StateUpdate {
        block_hash: BlockHash::default(),
        new_root: GlobalRoot::default(),
        old_root: GlobalRoot::default(),
        state_diff: apollo_starknet_client::reader::StateDiff {
            old_declared_contracts: class_hashes,
            ..Default::default()
        },
    }
}

// Whitebox test of the backpressure gate added to `handle_downloaded_state_updates`: it must
// hold a fully-downloaded state update (and its class hashes) back rather than admit it (or drop
// its hashes) once the class backlog reached the configured bound, and must resume admitting
// state updates once the backlog drains.
#[test]
fn class_backlog_backpressure_blocks_and_recovers() {
    let ((storage_reader, _storage_writer), _temp_dir) = get_test_storage();
    let class_cache = std::sync::Arc::new(std::sync::Mutex::new(lru::LruCache::new(
        NonZeroUsize::new(2).unwrap(),
    )));
    let apollo_starknet_client = std::sync::Arc::new(MockStarknetReader::new());

    // max_classes_to_store_in_memory == 3: exactly the size of the first update's class list.
    let mut stream = StateUpdateStream::new(
        BlockNumber(0),
        BlockNumber(0),
        apollo_starknet_client,
        storage_reader,
        test_config(3),
        class_cache,
    );
    let mut pinned_stream = Pin::new(&mut stream);

    let waker = futures::task::noop_waker_ref();
    let mut cx = std::task::Context::from_waker(waker);
    let mut should_poll_again = false;

    let first_update =
        state_update_with_classes(vec![class_hash!("0x1"), class_hash!("0x2"), class_hash!("0x3")]);
    pinned_stream
        .download_state_update_tasks
        .push_back(Box::pin(async move { (BlockNumber(0), Ok(Some(first_update))) }));
    pinned_stream.handle_downloaded_state_updates(&mut cx, &mut should_poll_again).unwrap();
    assert_eq!(pinned_stream.downloaded_state_updates.len(), 1);
    assert_eq!(pinned_stream.classes_to_download.len(), 3);

    // A second, fully-downloaded state update arrives while the backlog is already at the bound.
    // Backpressure must hold it back instead of admitting it (which would grow the backlog
    // unboundedly) or dropping its class hashes (which would desync sync).
    let second_update =
        state_update_with_classes(vec![class_hash!("0x4"), class_hash!("0x5"), class_hash!("0x6")]);
    pinned_stream
        .download_state_update_tasks
        .push_back(Box::pin(async move { (BlockNumber(1), Ok(Some(second_update))) }));
    pinned_stream.handle_downloaded_state_updates(&mut cx, &mut should_poll_again).unwrap();
    assert_eq!(
        pinned_stream.downloaded_state_updates.len(),
        1,
        "second state update should be held back by backpressure"
    );
    assert_eq!(
        pinned_stream.classes_to_download.len(),
        3,
        "no new hashes should be appended while the backlog is at its bound"
    );
    assert_eq!(
        pinned_stream.download_state_update_tasks.len(),
        1,
        "the held-back task must remain queued, not dropped"
    );

    // Draining the backlog (as scheduling class downloads normally would) makes room again.
    pinned_stream.classes_to_download.clear();
    pinned_stream.handle_downloaded_state_updates(&mut cx, &mut should_poll_again).unwrap();
    assert_eq!(
        pinned_stream.downloaded_state_updates.len(),
        2,
        "backpressure should release once the backlog drains"
    );
    assert_eq!(pinned_stream.classes_to_download.len(), 3);
}

// End-to-end test that a class backlog spanning multiple state updates never causes hashes to be
// dropped or the stream to deadlock, even with a small `max_classes_to_store_in_memory`.
#[tokio::test]
async fn class_backlog_backpressure_does_not_drop_or_deadlock() {
    let class_hashes = [
        class_hash!("0x1"),
        class_hash!("0x2"),
        class_hash!("0x3"),
        class_hash!("0x4"),
        class_hash!("0x5"),
        class_hash!("0x6"),
    ];

    let mut mock = MockStarknetReader::new();
    for (block_number, chunk) in class_hashes.chunks(2).enumerate() {
        let state_update = state_update_with_classes(chunk.to_vec());
        mock.expect_state_update()
            .with(predicate::eq(BlockNumber(u64::try_from(block_number).unwrap())))
            .times(1)
            .returning(move |_| Ok(Some(state_update.clone())));
    }
    for class_hash in class_hashes {
        mock.expect_class_by_hash().with(predicate::eq(class_hash)).times(1).returning(|_| {
            Ok(Some(GenericContractClass::Cairo0ContractClass(DeprecatedContractClass::default())))
        });
    }

    let ((storage_reader, _storage_writer), _temp_dir) = get_test_storage();
    let class_cache = std::sync::Arc::new(std::sync::Mutex::new(lru::LruCache::new(
        NonZeroUsize::new(2).unwrap(),
    )));

    // Only 2 classes may be held in memory at a time, forcing backpressure across all 3 blocks.
    let stream = StateUpdateStream::new(
        BlockNumber(0),
        BlockNumber(3),
        std::sync::Arc::new(mock),
        storage_reader,
        test_config(2),
        class_cache,
    );
    futures_util::pin_mut!(stream);

    for expected_block_number in 0..3u64 {
        let central_state_update = stream
            .next()
            .await
            .unwrap_or_else(|| panic!("stream ended early at block {expected_block_number}"))
            .unwrap_or_else(|err| {
                panic!("unexpected error at block {expected_block_number}: {err:?}")
            });
        assert_eq!(central_state_update.0, BlockNumber(expected_block_number));
    }
    assert!(stream.next().await.is_none(), "stream should be exhausted after 3 blocks");
}
