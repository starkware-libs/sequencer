use std::future::Future;

use apollo_metrics::metrics::MetricGauge;
#[cfg(feature = "os_input")]
use apollo_storage::accessed_keys::AccessedKeysStorageWriter;
use apollo_storage::base_layer::BaseLayerStorageWriter;
use apollo_storage::block_hash::BlockHashStorageWriter;
use apollo_storage::body::BodyStorageWriter;
use apollo_storage::class_manager::ClassManagerStorageWriter;
use apollo_storage::global_root::GlobalRootStorageWriter;
use apollo_storage::header::HeaderStorageWriter;
use apollo_storage::partial_block_hash::PartialBlockHashComponentsStorageWriter;
use apollo_storage::state::StateStorageWriter;
#[cfg(feature = "os_input")]
use apollo_storage::state_commitment_infos::StateCommitmentInfosStorageWriter;
use apollo_storage::StorageWriter;
use futures::future::pending;
use futures::never::Never;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use tracing::info;
use validator::{Validate, ValidationErrors};

/// The canonical config param name for [`RevertConfig`]. It is dumped (via `ser_optional_param`) as
/// a single optional leaf param under this name by every parent config, and is the shared pointer
/// target.
pub const REVERT_CONFIG_NAME: &str = "revert_config";
pub const REVERT_CONFIG_DESCRIPTION: &str = "The component will revert blocks up to and including \
                                             this block number. Use carefully to prevent \
                                             significant revert operations and data loss.";

/// The block number up to and including which the component will revert blocks. `None` means no
/// reverts are performed.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(transparent)]
pub struct RevertConfig(pub Option<u64>);

// Implemented manually because the `Validate` derive does not support tuple structs. There is
// nothing to validate.
impl Validate for RevertConfig {
    fn validate(&self) -> Result<(), ValidationErrors> {
        Ok(())
    }
}

pub struct RevertComponentData {
    pub name: &'static str,
    pub revert_metric: MetricGauge,
}

pub async fn revert_blocks_and_eternal_pending<Fut>(
    mut storage_height_marker: BlockNumber,
    revert_up_to_and_including: BlockNumber,
    mut revert_block_fn: impl FnMut(BlockNumber) -> Fut,
    component: &RevertComponentData,
) -> Never
where
    Fut: Future<Output = ()>,
{
    // If we revert all blocks up to height X (including), the new height marker will be X.
    let target_height_marker = revert_up_to_and_including;

    let RevertComponentData { name: component_name, revert_metric } = component;
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
        revert_metric.set_lossy(storage_height_marker.0);
        info!(
            "Successfully reverted {component_name}'s storage to height marker \
             {storage_height_marker}."
        );
        // Yield to the tokio runtime so other futures (e.g. the monitoring endpoint) can make
        // progress. Without this, callers whose revert closure resolves synchronously (like state
        // sync) would monopolize the executor for the entire loop, starving co-located tasks.
        tokio::task::yield_now().await;
    }

    info!("Done reverting {component_name}'s storage up to height {target_height_marker}!");
    match storage_height_marker.prev() {
        Some(latest_block_in_storage) => info!(
            "The latest block saved in {component_name}'s storage is {latest_block_in_storage}!"
        ),
        None => info!("There aren't any blocks saved in {component_name}'s storage!"),
    };
    info!("Starting eternal pending.");

    pending().await
}

/// Reverts everything related to the block, will succeed even if there is partial information for
/// the block.
// This function will panic if the storage reader fails to revert.
pub fn revert_block(storage_writer: &mut StorageWriter, target_block_marker: BlockNumber) {
    let txn = storage_writer
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
        .revert_partial_block_hash_components(&target_block_marker)
        .unwrap()
        .revert_block_hash(&target_block_marker)
        .unwrap()
        .revert_global_root(&target_block_marker)
        .unwrap();

    #[cfg(feature = "os_input")]
    let txn = txn
        .revert_accessed_keys(target_block_marker)
        .unwrap()
        .revert_state_commitment_infos(target_block_marker)
        .unwrap();

    txn.commit().unwrap();
}
