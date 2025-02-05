use std::future::Future;

use futures::future::pending;
use futures::never::Never;
use papyrus_storage::base_layer::BaseLayerStorageWriter;
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::class_manager::ClassManagerStorageWriter;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::StorageWriter;
use starknet_api::block::BlockNumber;
use tracing::info;

pub async fn revert_blocks_and_eternal_pending<Fut>(
    mut current_block_number: BlockNumber,
    revert_up_to_and_including: BlockNumber,
    mut revert_block_fn: impl FnMut(BlockNumber) -> Fut,
    component_name: &str,
) -> Never
where
    Fut: Future<Output = ()>,
{
    if current_block_number <= revert_up_to_and_including {
        panic!(
            "{component_name} current block {current_block_number} is not larger than the target \
             block {revert_up_to_and_including}. No reverts are needed."
        );
    }

    info!(
        "Reverting {component_name} from block {current_block_number} to block \
         {revert_up_to_and_including}"
    );

    while current_block_number > revert_up_to_and_including {
        current_block_number = current_block_number.prev().expect(
            "A block number that's greater than another block number should return Some on prev",
        );
        info!("Reverting {component_name} block number {current_block_number}.");
        revert_block_fn(current_block_number).await;
    }

    info!(
        "Done reverting {component_name} up to block {revert_up_to_and_including} including. \
         Starting eternal pending."
    );
    pending().await
}

pub fn revert_block(
    storage_writer: &mut StorageWriter,
    current_block_number: BlockNumber,
) -> impl Future<Output = ()> {
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .revert_header(current_block_number)
        .unwrap()
        .0
        .revert_body(current_block_number)
        .unwrap()
        .0
        .revert_state_diff(current_block_number)
        .unwrap()
        .0
        .try_revert_class_manager_marker(current_block_number)
        .unwrap()
        .try_revert_base_layer_marker(current_block_number)
        .unwrap()
        .commit()
        .unwrap();
    async {}
}
