use std::fs::create_dir_all;
use std::io::{self, Stdout, Write};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};

use apollo_config::CONFIG_FILE_ARG;
use apollo_infra_utils::command::create_shell_command;
use apollo_infra_utils::path::resolve_project_relative_path;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Child;
use tokio::task;
use tokio_util::task::AbortOnDropHandle;
use tracing::{error, info, instrument};

pub const NODE_EXECUTABLE_PATH: &str = "target/debug/apollo_node";
const TEMP_LOGS_DIR: &str = "integration_test_temporary_logs";

/// Global synchronized stdout writer to prevent race conditions when multiple
/// node processes write their annotated output concurrently.
static STDOUT_WRITER: OnceLock<Mutex<Stdout>> = OnceLock::new();

fn get_stdout_writer() -> &'static Mutex<Stdout> {
    STDOUT_WRITER.get_or_init(|| Mutex::new(io::stdout()))
}

/// Writes an annotated line to stdout atomically (with synchronization).
fn write_annotated_stdout_line(prefix: &str, line: &str) {
    let writer = get_stdout_writer();
    if let Ok(mut stdout) = writer.lock() {
        writeln!(stdout, "{} {}", prefix, line).expect("Should be able to write to stdout.");
        stdout.flush().expect("Should be able to flush stdout.");
    }
}

#[derive(Debug, Clone)]
pub struct NodeRunner {
    node_index: usize,
    node_execution_id: String,
}

impl NodeRunner {
    pub fn new(node_index: usize, node_execution_id: String) -> Self {
        create_dir_all(TEMP_LOGS_DIR).unwrap();
        Self { node_index, node_execution_id }
    }

    pub fn get_description(&self) -> String {
        format!("Node {} {}:", self.node_index, self.node_execution_id)
    }

    pub fn logs_file_path(&self) -> PathBuf {
        PathBuf::from(TEMP_LOGS_DIR)
            .join(format!("node_{}_{}.log", self.node_index, self.node_execution_id))
    }
}

pub fn spawn_run_node(
    node_config_paths: Vec<PathBuf>,
    node_runner: NodeRunner,
) -> AbortOnDropHandle<()> {
    AbortOnDropHandle::new(task::spawn(async move {
        info!("Running the node from its spawned task.");
        // Obtain handles, as the processes and task are terminated when their handles are dropped.
        let (mut node_handle, _pipe_task) =
            spawn_node_child_process(node_config_paths, node_runner.clone()).await;
        let _node_run_result = node_handle.
            wait(). // Runs the node until completion, should be running indefinitely.
            await; // Awaits the completion of the node.
        panic!("Node {node_runner:?} stopped unexpectedly.");
    }))
}

#[instrument(skip(node_runner))]
async fn spawn_node_child_process(
    node_config_paths: Vec<PathBuf>,
    node_runner: NodeRunner,
) -> (Child, AbortOnDropHandle<()>) {
    info!("Getting the node executable.");
    let node_executable = get_node_executable_path();

    let config_file_args: Vec<String> = node_config_paths
        .into_iter()
        .flat_map(|path| {
            let path_str = path.to_str().expect("Invalid path").to_string();
            vec![CONFIG_FILE_ARG.to_string(), path_str]
        })
        .collect();

    info!("Running the node from: {}", node_executable);
    let mut node_process: Child = create_shell_command(node_executable.as_str())
        .args(config_file_args)
        .stderr(Stdio::inherit())
        .stdout(Stdio::piped())
        .kill_on_drop(true) // Required for stopping when the handle is dropped.
        .spawn()
        .expect("Spawning sequencer node should succeed.");

    // Print the prefix in different colors.
    let prefix = format!(
        "\u{1b}[3{}m{}\u{1b}[0m",
        node_runner.node_index + 1,
        node_runner.get_description()
    );
    info!("Node PID: {:?}", node_process.id());

    // Get the node stdout.
    let node_stdout = node_process.stdout.take().expect("Node stdout should be available.");

    // Spawn a task to read node stdout and write to both file and synchronized stdout.
    let pipe_task = AbortOnDropHandle::new(tokio::spawn(async move {
        let mut reader = BufReader::new(node_stdout).lines();
        info!("Writing node logs to file: {:?}", node_runner.logs_file_path());
        let mut file =
            File::create(node_runner.logs_file_path()).await.expect("Failed to create log file.");
        while let Some(line) = reader.next_line().await.transpose() {
            match line {
                Ok(line) => {
                    // Write annotated line to synchronized stdout.
                    write_annotated_stdout_line(&prefix, &line);

                    // Write to file
                    if let Err(e) = file.write_all(line.as_bytes()).await {
                        error!("Failed to write to file: {}", e);
                    }
                    if let Err(e) = file.write_all(b"\n").await {
                        error!("Failed to write newline to file: {}", e);
                    }
                }
                Err(e) => {
                    error!("Error while reading node stdout: {}", e);
                }
            }
        }
    }));

    (node_process, pipe_task)
}

pub fn get_node_executable_path() -> String {
    resolve_project_relative_path(NODE_EXECUTABLE_PATH).map_or_else(
        |_| {
            error!(
                "Sequencer node binary is not present. Please compile it using 'cargo build --bin \
                 apollo_node' command."
            );
            panic!("Node executable should be available");
        },
        |path| path.to_string_lossy().to_string(),
    )
}
