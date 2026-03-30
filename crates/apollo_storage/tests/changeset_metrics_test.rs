// Integration test for changeset metric gauges.
// Runs in its own binary (separate process) so the global Prometheus recorder is isolated
// from the main unit-test binary — no interference from parallel tests that also mutate
// these gauges via append_state_diff / revert_state_diff.

use apollo_storage::db::DbConfig;
use apollo_storage::mmap_file::MmapFileConfig;
use apollo_storage::state::StateStorageWriter;
use apollo_storage::{open_storage, StorageConfig, StorageReader, StorageScope, StorageWriter};
use apollo_test_utils::prometheus_is_contained;
use indexmap::indexmap;
use metrics_exporter_prometheus::PrometheusBuilder;
use prometheus_parse::Value::Gauge;
use starknet_api::block::BlockNumber;
use starknet_api::state::ThinStateDiff;
use starknet_api::test_utils::CHAIN_ID_FOR_TESTS;
use starknet_api::{contract_address, storage_key};
use starknet_types_core::felt::Felt;
use tempfile::tempdir;

fn open_flat_state_storage() -> (StorageReader, StorageWriter) {
    let temp_dir = tempdir().unwrap();
    let config = StorageConfig {
        db_config: DbConfig {
            path_prefix: temp_dir.path().to_path_buf(),
            chain_id: CHAIN_ID_FOR_TESTS.clone(),
            enforce_file_exists: false,
            min_size: 1 << 20,
            max_size: 1 << 35,
            growth_step: 1 << 26,
            max_readers: 1 << 13,
        },
        scope: StorageScope::StateOnly,
        mmap_file_config: MmapFileConfig {
            max_size: 1 << 24,
            growth_step: 1 << 20,
            max_object_size: 1 << 16,
        },
        flat_state: true,
        changeset_retention_blocks: None,
    };
    // Leak temp_dir so the directory persists for the test duration.
    std::mem::forget(temp_dir);
    open_storage(config).unwrap()
}

#[test]
fn changeset_marker_metric_updates_on_append_and_revert() {
    let handle = PrometheusBuilder::new().install_recorder().unwrap();

    let (_reader, mut writer) = open_flat_state_storage();

    let address = contract_address!("0x1");
    let key = storage_key!("0x10");

    // Write 3 blocks.
    for block in 0u64..3 {
        let diff = ThinStateDiff {
            storage_diffs: indexmap! {
                address => indexmap! {
                    key => Felt::from(block + 1),
                },
            },
            ..Default::default()
        };
        writer
            .begin_rw_txn()
            .unwrap()
            .append_state_diff(BlockNumber(block), diff)
            .unwrap()
            .commit()
            .unwrap();
    }

    // After appending block 2, changeset marker should be 3 (next block without data).
    let Gauge(marker_value) =
        prometheus_is_contained(handle.render(), "batcher_storage_changeset_marker", &[]).unwrap()
    else {
        panic!("batcher_storage_changeset_marker is not a Gauge")
    };
    assert_eq!(marker_value, 3.0);

    // Revert block 2.
    let (txn, _) = writer.begin_rw_txn().unwrap().revert_state_diff(BlockNumber(2)).unwrap();
    txn.commit().unwrap();

    // After reverting block 2, changeset marker should be 2.
    let Gauge(marker_value) =
        prometheus_is_contained(handle.render(), "batcher_storage_changeset_marker", &[]).unwrap()
    else {
        panic!("batcher_storage_changeset_marker is not a Gauge")
    };
    assert_eq!(marker_value, 2.0);
}
