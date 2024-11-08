use std::env;
use std::path::PathBuf;
use std::process::Stdio;

use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use rstest::{fixture, rstest};
use starknet_integration_tests::integration_test_config_utils::dump_config_file_changes;
use starknet_integration_tests::integration_test_utils::{
    create_config,
    create_integration_test_tx_generator,
};
use starknet_integration_tests::state_reader::{spawn_test_rpc_state_reader, StorageTestSetup};
use starknet_sequencer_infra::trace_util::configure_tracing;
use tempfile::tempdir;
use tokio::process::{Child, Command};
use tokio::task::{self, JoinHandle};
use tracing::info;

#[fixture]
fn tx_generator() -> MultiAccountTransactionGenerator {
    create_integration_test_tx_generator()
}

// TODO(Tsabary): Move to a suitable util location.
async fn spawn_node_child_task(node_config_path: PathBuf) -> Child {
    // Get the current working directory for the project
    let project_path = env::current_dir().expect("Failed to get current directory").join("../..");

    // TODO(Tsabary): Change invocation from "cargo run" to separate compilation and invocation
    // (build, and then invoke the binary).
    Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("starknet_sequencer_node")
        .arg("--quiet")
        .current_dir(&project_path)
        .arg("--")
        .arg("--config_file")
        .arg(node_config_path.to_str().unwrap())
        .stderr(Stdio::inherit())
        .stdout(Stdio::null())
        .kill_on_drop(true) // Required for stopping the node when the handle is dropped.
        .spawn()
        .expect("Failed to spawn the sequencer node.")
}

async fn spawn_run_node(node_config_path: PathBuf) -> JoinHandle<()> {
    task::spawn(async move {
        info!("Running the node from its spawned task.");
        let _node_run_result = spawn_node_child_task(node_config_path).
            await. // awaits the completion of spawn_node_child_task.
            wait(). // runs the node until completion -- should be running indefinitely.
            await; // awaits the completion of the node.
        panic!("Node stopped unexpectedly.");
    })
}

#[rstest]
#[tokio::test]
async fn test_end_to_end_integration(tx_generator: MultiAccountTransactionGenerator) {
    configure_tracing();
    info!("Running integration test setup.");

    // Creating the storage for the test.
    let storage_for_test = StorageTestSetup::new(tx_generator.accounts());

    // Spawn a papyrus rpc server for a papyrus storage reader.
    let rpc_server_addr =
        spawn_test_rpc_state_reader(storage_for_test.rpc_storage_reader, storage_for_test.chain_id)
            .await;

    // Derive the configuration for the sequencer node.
    let (config, required_params) =
        create_config(rpc_server_addr, storage_for_test.batcher_storage_config).await;

    // Note: the batcher storage file handle is passed as a reference to maintain its ownership in
    // this scope, such that the handle is not dropped and the storage is maintained.
    let temp_dir = tempdir().unwrap();
    // TODO(Tsabary): pass path instead of temp dir.
    let (node_config_path, _) = dump_config_file_changes(&config, required_params, &temp_dir);

    info!("Running sequencer node.");
    let _node_run_handle = spawn_run_node(node_config_path).await;
}
