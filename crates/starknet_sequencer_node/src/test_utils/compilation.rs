use std::io;
use std::process::{ExitStatus, Stdio};

use infra_utils::command::create_shell_command;
use tracing::info;

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
