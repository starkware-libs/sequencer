use std::fs::create_dir_all;
use std::path::PathBuf;
use std::process::Stdio;

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

#[derive(Debug, Clone)]
pub struct NodeRunner {
    node_index: usize,
    executable_index: usize,
}

impl NodeRunner {
    pub fn new(node_index: usize, executable_index: usize) -> Self {
        create_dir_all(TEMP_LOGS_DIR).unwrap();
        Self { node_index, executable_index }
    }

    pub fn get_description(&self) -> String {
        format!("Node id {} part {}:", self.node_index, self.executable_index)
    }

    pub fn logs_file_path(&self) -> PathBuf {
        PathBuf::from(TEMP_LOGS_DIR)
            .join(format!("node_{}_part_{}.log", self.node_index, self.executable_index))
    }
}

pub fn spawn_run_node(
    node_config_paths: Vec<PathBuf>,
    node_runner: NodeRunner,
) -> AbortOnDropHandle<()> {
    AbortOnDropHandle::new(task::spawn(async move {
        info!("Running the node from its spawned task.");
        // Obtain handles, as the processes and task are terminated when their handles are dropped.
        let (mut node_handle, _annotator_handle, _pipe_task) =
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
) -> (Child, Child, AbortOnDropHandle<()>) {
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

    let mut annotator_process: Child = create_shell_command("awk")
        .arg("-v")
        // Print the prefix in different colors.
        .arg(format!("prefix=\u{1b}[3{}m{}\u{1b}[0m", node_runner.node_index+1, node_runner.get_description()))
        .arg("{print prefix, $0}")
        .stdin(std::process::Stdio::piped())
        .stderr(Stdio::inherit())
        .stdout(Stdio::inherit())
        .kill_on_drop(true) // Required for stopping when the handle is dropped.
        .spawn()
        .expect("Spawning node output annotation should succeed.");

    info!("Node PID: {:?}, Annotator PID: {:?}", node_process.id(), annotator_process.id());

    // Get the node stdout and the annotator stdin.
    let node_stdout = node_process.stdout.take().expect("Node stdout should be available.");
    let mut annotator_stdin =
        annotator_process.stdin.take().expect("Annotator stdin should be available.");

    // Spawn a task to connect the node stdout with the annotator stdin.
    let pipe_task = AbortOnDropHandle::new(tokio::spawn(async move {
        let mut reader = BufReader::new(node_stdout).lines();
        info!("Writing node logs to file: {:?}", node_runner.logs_file_path());
        let mut file =
            File::create(node_runner.logs_file_path()).await.expect("Failed to create log file.");
        while let Some(line) = reader.next_line().await.transpose() {
            match line {
                Ok(line) => {
                    // Write to annotator stdin
                    if let Err(e) = annotator_stdin.write_all(line.as_bytes()).await {
                        error!("Failed to write to annotator stdin: {}", e);
                    }
                    if let Err(e) = annotator_stdin.write_all(b"\n").await {
                        error!("Failed to write newline to annotator stdin: {}", e);
                    }

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

        // Close the annotator stdin when done.
        if let Err(e) = annotator_stdin.shutdown().await {
            error!("Failed to shut down annotator stdin: {}", e);
        }
    }));

    (node_process, annotator_process, pipe_task)
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
