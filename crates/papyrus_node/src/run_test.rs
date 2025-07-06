use std::time::Duration;

use apollo_storage::{open_storage, StorageConfig};
use apollo_test_utils::prometheus_is_contained;
use metrics_exporter_prometheus::PrometheusBuilder;
use tempfile::TempDir;

use crate::config::NodeConfig;
use crate::run::{
    run_threads,
    spawn_storage_metrics_collector,
    PapyrusResources,
    PapyrusTaskHandles,
};

// The mission of this test is to ensure that if an error is returned from one of the spawned tasks,
// the node will stop, and this error will be returned. This is done by checking the case of a
// network handler that returns an error, which will cause the sync task to return an error.
#[tokio::test]
async fn run_threads_stop() {
    let mut config = NodeConfig::default();
    let temp_dir = TempDir::new().unwrap();
    config.storage.db_config.path_prefix = temp_dir.path().into();

    let resources = PapyrusResources::new(&config).unwrap();
    let tasks = PapyrusTaskHandles {
        network_handle: Some(tokio::task::spawn(async {
            tokio::time::sleep(Duration::from_secs(1)).await;
            Err(anyhow::Error::msg("Network task stopped"))
        })),
        ..Default::default()
    };
    let error = run_threads(config, resources, tasks).await.expect_err("Should be an error.");
    assert_eq!("Network task stopped", error.to_string());
}

// TODO(dvir): use here metrics names from the storage instead of hard-coded ones. This will be done
// only after changes to the metrics structure in papyrus.
#[tokio::test]
async fn storage_metrics_collector() {
    let mut storage_config = StorageConfig::default();
    let temp_dir = TempDir::new().unwrap();
    storage_config.db_config.path_prefix = temp_dir.path().into();
    let (storage_reader, _storage_writer) = open_storage(storage_config).unwrap();
    let handle = PrometheusBuilder::new().install_recorder().unwrap();

    assert!(prometheus_is_contained(handle.render(), "storage_free_pages_number", &[]).is_none());

    spawn_storage_metrics_collector(true, storage_reader, Duration::from_secs(1));
    // To make sure the metrics in the spawned thread are updated.
    tokio::time::sleep(Duration::from_millis(1)).await;

    assert!(prometheus_is_contained(handle.render(), "storage_free_pages_number", &[]).is_some());
}
