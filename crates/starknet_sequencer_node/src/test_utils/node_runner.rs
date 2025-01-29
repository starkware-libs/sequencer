use std::path::PathBuf;
use std::process::Stdio;

use starknet_infra_utils::command::create_shell_command;
use starknet_infra_utils::path::resolve_project_relative_path;
use tokio::process::Child;
use tokio::task::{self, JoinHandle};
use tracing::{error, info, instrument};

pub const NODE_EXECUTABLE_PATH: &str = "target/debug/starknet_sequencer_node";

pub fn spawn_run_node(node_config_path: PathBuf) -> JoinHandle<()> {
    task::spawn(async move {
        info!("Running the node from its spawned task.");
        let _node_run_result = spawn_node_child_process(node_config_path).
            await. // awaits the completion of spawn_node_child_task.
            wait(). // runs the node until completion -- should be running indefinitely.
            await; // awaits the completion of the node.
        panic!("Node stopped unexpectedly.");
    })
}

#[instrument()]
async fn spawn_node_child_process(node_config_path: PathBuf) -> Child {
    // TODO(Tsabary): Capture output to a log file, and present it in case of a failure.
    info!("Getting the node executable.");
    let node_executable = get_node_executable_path();

    info!("Running the node from: {}", node_executable);
    create_shell_command(node_executable.as_str())
        .arg("--config_file")
        .arg(node_config_path.to_str().unwrap())
        .stderr(Stdio::inherit())
        .stdout(Stdio::inherit())
        .kill_on_drop(true) // Required for stopping the node when the handle is dropped.
        .spawn()
        .expect("Failed to spawn the sequencer node.")
}

pub fn get_node_executable_path() -> String {
    resolve_project_relative_path(NODE_EXECUTABLE_PATH).map_or_else(
        |_| {
            error!(
                "Sequencer node binary is not present. Please compile it using 'cargo build --bin \
                 starknet_sequencer_node' command."
            );
            panic!("Node executable should be available");
        },
        |path| path.to_string_lossy().to_string(),
    )
}
