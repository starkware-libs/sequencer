use std::io;
use std::process::{Command, ExitStatus, Stdio};

use infra_utils::path::resolve_project_relative_path;
use tracing::info;

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
fn compile_node() -> io::Result<ExitStatus> {
    info!("Compiling the starknet_sequencer_node binary");
    let project_path = resolve_project_relative_path(".").expect("Failed to resolve project path");
    info!("project_path {:?}", project_path);

    // Run `cargo build` to compile the project
    let compilation_result = Command::new("cargo")
        .arg("build")
        .arg("--bin")
        .arg("starknet_sequencer_node")
        .current_dir(&project_path)
        // .arg("--quiet")
        .stderr(Stdio::inherit())
        .stdout(Stdio::inherit())
        .status();

    info!("Compilation result: {:?}", compilation_result);
    compilation_result
}

pub fn compile_node_result() -> Result<(), NodeCompilationError> {
    match compile_node() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(NodeCompilationError::Status(status)),
        Err(e) => Err(NodeCompilationError::IO(e)),
    }
}
