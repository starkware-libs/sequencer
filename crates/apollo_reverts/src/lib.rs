use std::collections::BTreeMap;
use std::future::Future;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_storage::base_layer::BaseLayerStorageWriter;
use apollo_storage::body::BodyStorageWriter;
use apollo_storage::class_manager::ClassManagerStorageWriter;
use apollo_storage::header::HeaderStorageWriter;
use apollo_storage::state::StateStorageWriter;
use apollo_storage::StorageWriter;
use futures::future::pending;
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
) where
    Fut: Future<Output = ()>,
{
    // If we revert all blocks up to height X (including), the new height marker will be X.
    let target_height_marker = revert_up_to_and_including;

    if storage_height_marker <= target_height_marker {
        info!(
            "{component_name}'s storage height marker {storage_height_marker} is not larger than \
             the target height marker {target_height_marker}. No reverts are needed."
        );
    } else {
        info!(
            "Reverting {component_name}'s storage from height marker {storage_height_marker} to \
             target height marker {target_height_marker}"
        );
    }

    while storage_height_marker > target_height_marker {
        storage_height_marker = storage_height_marker.prev().expect(
            "A block number that's greater than another block number should return Some on prev",
        );
        info!("Reverting {component_name}'s storage to height marker {storage_height_marker}.");
        revert_block_fn(storage_height_marker).await;
        info!(
            "Successfully reverted {component_name}'s storage to height marker \
             {storage_height_marker}."
        );
    }

    info!("Done reverting {component_name}'s storage up to height {target_height_marker}!");
    match storage_height_marker.prev() {
        Some(latest_block_in_storage) => info!(
            "The latest block saved in {component_name}'s storage is {latest_block_in_storage}!"
        ),
        None => info!("There aren't any blocks saved in {component_name}'s storage!"),
    };
    info!("Starting eternal pending.");
    if component_name == "State Sync" {
        pending().await
    }
}

/// Reverts everything related to the block, will succeed even if there is partial information for
/// the block.
// This function will panic if the storage reader fails to revert.
pub fn revert_block(storage_writer: &mut StorageWriter, target_block_marker: BlockNumber) {
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .revert_header(target_block_marker)
        .unwrap()
        .0
        .revert_body(target_block_marker)
        .unwrap()
        .0
        .revert_state_diff(target_block_marker)
        .unwrap()
        .0
        .try_revert_class_manager_marker(target_block_marker)
        .unwrap()
        .try_revert_base_layer_marker(target_block_marker)
        .unwrap()
        .commit()
        .unwrap();
}
