use std::io;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};

use infra_utils::command::create_shell_command;
use infra_utils::path::resolve_project_relative_path;
use tokio::process::Child;
use tokio::task::{self, JoinHandle};
use tracing::{error, info};

pub const NODE_EXECUTABLE_PATH: &str = "target/debug/starknet_sequencer_node";

#[cfg(test)]
#[path = "compilation_test.rs"]
mod compilation_test;

#[derive(thiserror::Error, Debug)]
pub enum NodeCompilationError {
    #[error(transparent)]
    IO(#[from] io::Error),
    #[error("Exit status: {0}.")]
    Status(ExitStatus),
}

/// Compiles the node using `cargo build` for testing purposes.
async fn compile_node() -> io::Result<ExitStatus> {
    info!(
        "Compiling the starknet_sequencer_node binary, expected destination: \
         {NODE_EXECUTABLE_PATH}"
    );

    // Run `cargo build` to compile the project
    let compilation_result = create_shell_command("cargo")
        .arg("build")
        .arg("--bin")
        .arg("starknet_sequencer_node")
        .arg("--quiet")
        .stderr(Stdio::inherit())
        .stdout(Stdio::inherit())
        .status()
        .await?;

    info!("Compilation result: {:?}", compilation_result);
    Ok(compilation_result)
}

pub async fn compile_node_result() -> Result<(), NodeCompilationError> {
    match compile_node().await {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(NodeCompilationError::Status(status)),
        Err(e) => Err(NodeCompilationError::IO(e)),
    }
}

pub async fn spawn_run_node(node_config_path: PathBuf) -> JoinHandle<()> {
    task::spawn(async move {
        info!("Running the node from its spawned task.");
        let _node_run_result = spawn_node_child_task(node_config_path).
            await. // awaits the completion of spawn_node_child_task.
            wait(). // runs the node until completion -- should be running indefinitely.
            await; // awaits the completion of the node.
        panic!("Node stopped unexpectedly.");
    })
}

async fn spawn_node_child_task(node_config_path: PathBuf) -> Child {
    // TODO(Tsabary): Capture output to a log file, and present it in case of a failure.
    info!("Compiling the node.");
    compile_node_result().await.expect("Failed to compile the sequencer node.");
    let node_executable = resolve_project_relative_path(NODE_EXECUTABLE_PATH)
        .expect("Node executable should be available")
        .to_string_lossy()
        .to_string();

    info!("Running the node from: {}", node_executable);
    create_shell_command(node_executable.as_str())
        .arg("--config_file")
        .arg(node_config_path.to_str().unwrap())
        .stderr(Stdio::inherit())
        .stdout(Stdio::null())
        .kill_on_drop(true) // Required for stopping the node when the handle is dropped.
        .spawn()
        .expect("Failed to spawn the sequencer node.")
}
