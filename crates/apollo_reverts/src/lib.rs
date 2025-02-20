use std::collections::BTreeMap;
use std::future::Future;

use futures::future::pending;
use futures::never::Never;
use papyrus_config::dumping::{ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_storage::base_layer::BaseLayerStorageWriter;
use papyrus_storage::body::BodyStorageWriter;
use papyrus_storage::class_manager::ClassManagerStorageWriter;
use papyrus_storage::header::HeaderStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use papyrus_storage::StorageWriter;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use tracing::info;
use validator::Validate;

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct RevertConfig {
    pub revert_up_to_and_including: BlockNumber,
    pub should_revert: bool,
}

impl Default for RevertConfig {
    fn default() -> Self {
        Self {
            // Use u64::MAX as a placeholder to prevent setting this value to
            // a low block number by mistake, which will cause significant revert operations.
            revert_up_to_and_including: BlockNumber(u64::MAX),
            should_revert: false,
        }
    }
}

impl SerializeConfig for RevertConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "revert_up_to_and_including",
                &self.revert_up_to_and_including,
                "The component will revert blocks up to this block number (including).",
                // Use this configuration carefully to prevent significant revert operations and
                // data loss
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "should_revert",
                &self.should_revert,
                "If set true, the component would revert blocks and do nothing else.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

pub async fn revert_blocks_and_eternal_pending<Fut>(
    mut storage_height_marker: BlockNumber,
    revert_up_to_and_including: BlockNumber,
    mut revert_block_fn: impl FnMut(BlockNumber) -> Fut,
    component_name: &str,
) -> Never
where
    Fut: Future<Output = ()>,
{
    if storage_height_marker <= revert_up_to_and_including {
        panic!(
            "{component_name}'s storage height marker {storage_height_marker} is not larger than \
             the target block {revert_up_to_and_including}. No reverts are needed."
        );
    }

    info!(
        "Reverting {component_name} from storage height marker {storage_height_marker} to target \
         storage height marker {revert_up_to_and_including}"
    );

    while storage_height_marker > revert_up_to_and_including {
        storage_height_marker = storage_height_marker.prev().expect(
            "A block number that's greater than another block number should return Some on prev",
        );
        info!("Reverting {component_name}'s storage to height {storage_height_marker}.");
        revert_block_fn(storage_height_marker).await;
        info!(
            "Successfully reverted {component_name}'s storage to height {storage_height_marker}."
        );
    }

    info!(
        "Done reverting {component_name} up to height {revert_up_to_and_including}. The latest \
         block in storage is {}.
         Starting eternal pending.",
        revert_up_to_and_including.0 - 1
    );
    pending().await
}

/// Reverts everything related to the block, will succeed even if there is partial information for
/// the block.
// This function will panic if the storage reader fails to revert.
pub fn revert_block(storage_writer: &mut StorageWriter, current_block_number: BlockNumber) {
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
}
