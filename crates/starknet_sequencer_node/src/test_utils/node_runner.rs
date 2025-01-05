use std::path::PathBuf;
use std::process::Stdio;

use infra_utils::command::create_shell_command;
use infra_utils::path::resolve_project_relative_path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Child;
use tokio::task::{self, JoinHandle};
use tracing::{error, info, instrument};
pub const NODE_EXECUTABLE_PATH: &str = "target/debug/starknet_sequencer_node";

pub fn spawn_run_node(node_config_path: PathBuf) -> JoinHandle<()> {
    task::spawn(async move {
        info!("Running the node from its spawned task.");
        // Obtain both handles, as the processes are terminated when their handles are dropped.
        let (mut node_handle, _annotator_handle) =
            spawn_node_child_process(node_config_path).await;
        let _node_run_result = node_handle.
            wait(). // Runs the node until completion, should be running indefinitely.
            await; // Awaits the completion of the node.
        panic!("Node stopped unexpectedly.");
    })
}

#[instrument()]
async fn spawn_node_child_process(node_config_path: PathBuf) -> (Child, Child) {
    info!("Getting the node executable.");
    let node_executable = get_node_executable_path();

    info!("Running the node from: {}", node_executable);
    let mut node_cmd: Child = create_shell_command(node_executable.as_str())
        .arg("--config_file")
        .arg(node_config_path.to_str().unwrap())
        .stderr(Stdio::inherit())
        .stdout(Stdio::piped())
        .kill_on_drop(true) // Required for stopping when the handle is dropped.
        .spawn()
        .expect("Spawning sequencer node should succeed.");

    let mut annotator_cmd: Child = create_shell_command("awk")
        .arg("{print $0}")
        .stdin(std::process::Stdio::piped())
        .stderr(Stdio::inherit())
        .stdout(Stdio::inherit())
        .kill_on_drop(true) // Required for stopping when the handle is dropped.
        .spawn()
        .expect("Spawning node output annotation should succeed.");

    // Get the node stdout and the annotator stdin.
    let node_stdout = node_cmd.stdout.take().expect("Node stdout should be available.");
    let mut annotator_stdin =
        annotator_cmd.stdin.take().expect("Processing stdin should be available.");

    // Spawn a task to connect the node with the annotator.
    tokio::spawn(async move {
        let mut reader = BufReader::new(node_stdout);
        let mut buffer = String::new();

        while let Ok(bytes_read) = reader.read_line(&mut buffer).await {
            if bytes_read == 0 {
                break; // End of input.
            }

            if annotator_stdin.write_all(buffer.as_bytes()).await.is_err() {
                error!("Failed to write to annotator stdin.");
                break;
            }

            buffer.clear(); // Clear the buffer for the next line.
        }

        // Close the annotator stdin when done.
        let _ = annotator_stdin.shutdown().await;
    });

    (node_cmd, annotator_cmd)
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
