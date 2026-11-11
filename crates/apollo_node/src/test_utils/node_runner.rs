use std::fs::create_dir_all;
use std::io::{self, stdout, Stdout, Write};
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::{Mutex, OnceLock};

use apollo_config::CONFIG_FILE_ARG;
use apollo_infra_utils::command::create_shell_command;
use apollo_infra_utils::path::resolve_project_relative_path;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Child;
use tokio::task::{self, JoinError};
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;
use tracing::{error, info, instrument};

#[cfg(test)]
#[path = "node_runner_test.rs"]
mod node_runner_test;

pub const NODE_EXECUTABLE_PATH: &str = "target/debug/apollo_node";
const TEMP_LOGS_DIR: &str = "integration_test_temporary_logs";
/// The SIGKILL signal number, the same on Linux and macOS.
const SIGKILL: i32 = 9;

/// Global synchronized stdout writer to prevent race conditions when multiple
/// node processes write their annotated output concurrently.
static STDOUT_WRITER: OnceLock<Mutex<Stdout>> = OnceLock::new();

fn get_stdout_writer() -> &'static Mutex<Stdout> {
    STDOUT_WRITER.get_or_init(|| Mutex::new(stdout()))
}

/// Writes an annotated line to stdout atomically (with synchronization).
fn write_annotated_stdout_line(prefix: &str, line: &str) {
    let writer = get_stdout_writer();
    if let Ok(mut stdout) = writer.lock() {
        writeln!(stdout, "{} {}", prefix, line).expect("Should be able to write to stdout.");
        stdout.flush().expect("Should be able to flush stdout.");
    }
}

/// Writes a line with a newline to an async file. Clears the file handle on failure so subsequent
/// calls are no-ops, preventing a panic from writing to a file whose background task has failed.
async fn write_file_line(file: &mut Option<File>, line: &str) {
    if let Some(f) = file.as_mut() {
        if let Err(e) = f.write_all(format!("{line}\n").as_bytes()).await {
            error!("Failed to write to file: {}", e);
            *file = None;
        }
    }
}

/// Handle to the task running a node process. Dropping it kills the process without waiting for it
/// to exit.
pub struct NodeProcessHandle {
    task: AbortOnDropHandle<()>,
    shutdown_token: CancellationToken,
}

impl NodeProcessHandle {
    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }

    /// Kills the node process and returns once the process exited, so its sockets are closed.
    /// Returns an error if the task panicked, as it does when the node stops on its own.
    pub async fn shutdown(self) -> Result<(), JoinError> {
        self.shutdown_token.cancel();
        self.task.await
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
) -> NodeProcessHandle {
    spawn_run_node_executable(get_node_executable_path(), node_config_paths, node_runner)
}

fn spawn_run_node_executable(
    node_executable: String,
    node_config_paths: Vec<PathBuf>,
    node_runner: NodeRunner,
) -> NodeProcessHandle {
    let shutdown_token = CancellationToken::new();
    let task_shutdown_token = shutdown_token.clone();
    let task = AbortOnDropHandle::new(task::spawn(async move {
        info!("Running the node from its spawned task.");
        // Obtain handles, as the processes and task are terminated when their handles are dropped.
        let (mut node_process, _pipe_task) =
            spawn_node_child_process(&node_executable, &node_config_paths, node_runner.clone())
                .await;
        let Some(exit_status) =
            wait_for_exit_or_shutdown(&mut node_process, &task_shutdown_token).await
        else {
            info!("Node {node_runner:?} has been shut down.");
            return;
        };
        panic!("Node {node_runner:?} stopped unexpectedly. Exit status: {exit_status:?}.");
    }));
    NodeProcessHandle { task, shutdown_token }
}

/// Waits for the node to exit, killing it once `shutdown_token` is cancelled. Returns `None` if the
/// kill ended the node, and the node's exit status otherwise.
async fn wait_for_exit_or_shutdown(
    node_process: &mut Child,
    shutdown_token: &CancellationToken,
) -> Option<io::Result<ExitStatus>> {
    tokio::select! {
        exit_status = node_process.wait() => Some(exit_status),
        () = shutdown_token.cancelled() => {
            // `wait` lags an exit until the runtime's driver processes it, so check it directly.
            if let Some(exit_status) = node_process.try_wait().transpose() {
                return Some(exit_status);
            }
            let exit_status = match node_process.start_kill() {
                Ok(()) => node_process.wait().await,
                Err(error) => Err(error),
            };
            unexpected_exit_after_kill(exit_status)
        }
    }
}

/// Returns `exit_status` unless it is the SIGKILL termination that the shutdown kill causes.
fn unexpected_exit_after_kill(
    exit_status: io::Result<ExitStatus>,
) -> Option<io::Result<ExitStatus>> {
    match exit_status {
        Ok(status) if status.signal() == Some(SIGKILL) => None,
        unexpected_exit_status => Some(unexpected_exit_status),
    }
}

#[instrument(skip(node_runner))]
async fn spawn_node_child_process(
    node_executable: &str,
    node_config_paths: &[PathBuf],
    node_runner: NodeRunner,
) -> (Child, AbortOnDropHandle<()>) {
    // The node loads its config natively: the paths are a nested base config followed by a flat
    // secrets file, matching `load_native`'s `[base, secret]` arity.
    let config_file_args: Vec<String> = node_config_paths
        .iter()
        .flat_map(|path| {
            let path_str = path.to_str().expect("Invalid path").to_string();
            vec![CONFIG_FILE_ARG.to_string(), path_str]
        })
        .collect();

    info!("Running the node from: {}", node_executable);
    let mut node_process: Child = create_shell_command(node_executable)
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
        let mut file = Some(
            File::create(node_runner.logs_file_path()).await.expect("Failed to create log file."),
        );
        while let Some(line) = reader.next_line().await.transpose() {
            match line {
                Ok(line) => {
                    // Blocking: acquires a global mutex and flushes stdout. Run on the blocking
                    // thread pool to avoid starving the async runtime when many nodes log
                    // simultaneously.
                    let prefix_clone = prefix.clone();
                    let line_clone = line.clone();
                    task::spawn_blocking(move || {
                        write_annotated_stdout_line(&prefix_clone, &line_clone);
                    })
                    .await
                    .ok();

                    // Write to file.
                    write_file_line(&mut file, &line).await;
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
