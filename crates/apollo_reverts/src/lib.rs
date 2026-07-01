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
use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use starknet_api::block::BlockNumber;
use tracing::info;
use validator::Validate;

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct RevertConfig {
    #[serde(deserialize_with = "deserialize_revert_block_number")]
    pub revert_up_to_and_including: BlockNumber,
    pub should_revert: bool,
}

/// Deserialize `revert_up_to_and_including`, tolerating a floating-point representation of the
/// value.
///
/// The default is the `u64::MAX` "never revert" sentinel. Configs assembled via jsonnet (the
/// `native` config format) render every number as an IEEE-754 double, so `u64::MAX`
/// (18446744073709551615) is emitted as the nearest double, `2^64` (18446744073709551616), which
/// overflows `u64` and would otherwise fail to deserialize. We accept a float and saturating-cast
/// it back to `u64`, so a config produced by jsonnet deserializes to the same `BlockNumber` as the
/// legacy flat (`preset`) path, which carries the exact `u64::MAX` integer. Plain integer values
/// (the common case, including real revert heights) take the `u64` branch unchanged.
fn deserialize_revert_block_number<'de, D>(deserializer: D) -> Result<BlockNumber, D::Error>
where
    D: Deserializer<'de>,
{
    struct RevertBlockNumberVisitor;

    impl<'de> Visitor<'de> for RevertBlockNumberVisitor {
        type Value = BlockNumber;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(
                "a block number; jsonnet renders the u64::MAX sentinel as the 2^64 value, which \
                 deserializers may present as u128 or f64",
            )
        }

        fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
            Ok(BlockNumber(value))
        }

        // jsonnet's f64 rounding turns u64::MAX into 2^64, which serde_json surfaces as a u128.
        // Saturate anything past u64::MAX back to the sentinel.
        fn visit_u128<E: de::Error>(self, value: u128) -> Result<Self::Value, E> {
            Ok(BlockNumber(u64::try_from(value).unwrap_or(u64::MAX)))
        }

        fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
            Ok(BlockNumber(u64::try_from(value).unwrap_or(0)))
        }

        // Saturating float-to-int cast: 2^64 (and larger) maps to u64::MAX.
        fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
            Ok(BlockNumber(value as u64))
        }
    }

    deserializer.deserialize_any(RevertBlockNumberVisitor)
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
