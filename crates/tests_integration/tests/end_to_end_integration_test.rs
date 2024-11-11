use std::env;
use std::path::PathBuf;
use std::process::Stdio;

use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use rstest::{fixture, rstest};
use starknet_integration_tests::integration_test_setup::IntegrationTestSetup;
use starknet_integration_tests::utils::{
    create_integration_test_tx_generator,
    run_transaction_generator_test_scenario,
};
use starknet_sequencer_infra::trace_util::configure_tracing;
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

    let integration_test_setup = IntegrationTestSetup::new_from_tx_generator(&tx_generator).await;

    info!("Running sequencer node.");
    let node_run_handle = spawn_run_node(integration_test_setup.node_config_path).await;

    // Wait for the node to start.
    integration_test_setup.is_alive_test_client.await_alive().await;

    info!("Running integration test simulator.");

<<<<<<< HEAD
    let send_rpc_tx_fn =
        &mut |rpc_tx| integration_test_setup.add_tx_http_client.assert_add_tx_success(rpc_tx);

=======
    let send_rpc_tx_fn = &mut |rpc_tx| http_test_client.assert_add_tx_success(rpc_tx);
>>>>>>> d24924dfe (chore: changing closure Fn requirements on integration test utils)
    let n_txs = 50;
    info!("Sending {n_txs} txs.");
    run_transaction_generator_test_scenario(tx_generator, n_txs, send_rpc_tx_fn).await;

    info!("Shutting down.");
    node_run_handle.abort();
    let res = node_run_handle.await;
    assert!(
        res.expect_err("Node should have been stopped.").is_cancelled(),
        "Node should have been stopped."
    );
}
