use std::fs::create_dir_all;
use std::io::{self, stderr, stdout, Stdout, Write};
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use apollo_config::CONFIG_FILE_ARG;
use apollo_infra_utils::command::create_shell_command;
use apollo_infra_utils::path::resolve_project_relative_path;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Child;
use tokio::task::{self, JoinError};
use tokio::time::{sleep, timeout};
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;
use tracing::{error, info, instrument, warn};

#[cfg(test)]
#[path = "node_runner_test.rs"]
mod node_runner_test;

pub const NODE_EXECUTABLE_PATH: &str = "target/debug/apollo_node";
const TEMP_LOGS_DIR: &str = "integration_test_temporary_logs";

/// Delays before each restart of a node that failed to bind an address already in use. They sum to
/// 70s, past the 60s TIME_WAIT that keeps a closed connection's source port bound.
const NODE_RESTART_BACKOFFS: [Duration; 4] = [
    Duration::from_secs(2),
    Duration::from_secs(8),
    Duration::from_secs(20),
    Duration::from_secs(40),
];

/// In the node's panic message when a listener's bind panics on an address already in use.
const ADDRESS_IN_USE_MESSAGE: &str = "Address already in use";

/// Bounds the wait for each of the node's output pipes to reach end of file after the node exits.
const PIPE_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);

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
    /// Returns an error if the task panicked, as it does when the node stops on its own and is not
    /// restarted.
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

/// Runs the node in a task that panics when the node exits on its own. A node that exits after
/// failing to bind an address already in use is first restarted, after each delay in
/// `NODE_RESTART_BACKOFFS`. Listeners bind only while the node starts, so a node that has been up
/// is never restarted.
pub fn spawn_run_node(
    node_config_paths: Vec<PathBuf>,
    node_runner: NodeRunner,
) -> NodeProcessHandle {
    spawn_run_node_executable(
        get_node_executable_path(),
        node_config_paths,
        node_runner,
        &NODE_RESTART_BACKOFFS,
    )
}

fn spawn_run_node_executable(
    node_executable: String,
    node_config_paths: Vec<PathBuf>,
    node_runner: NodeRunner,
    restart_backoffs: &'static [Duration],
) -> NodeProcessHandle {
    let shutdown_token = CancellationToken::new();
    let task_shutdown_token = shutdown_token.clone();
    let task = AbortOnDropHandle::new(task::spawn(async move {
        info!("Running the node from its spawned task.");
        let Some(exit_status) = run_node_until_fatal_exit(
            &node_executable,
            &node_config_paths,
            &node_runner,
            restart_backoffs,
            &task_shutdown_token,
        )
        .await
        else {
            info!("Node {node_runner:?} has been shut down.");
            return;
        };
        panic!("Node {node_runner:?} stopped unexpectedly. Exit status: {exit_status:?}.");
    }));
    NodeProcessHandle { task, shutdown_token }
}

/// Returns the node's exit status if it exited before `shutdown_token` is cancelled. Otherwise
/// kills the node and returns `None` once it exited.
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
            node_process.kill().await.expect("Killing the node should succeed.");
            None
        }
    }
}

/// Returns the exit status of the first exit that is not an address-in-use bind failure, or of the
/// last attempt once every delay in `restart_backoffs` was used. Returns `None` if `shutdown_token`
/// is cancelled first, once no node process is running.
async fn run_node_until_fatal_exit(
    node_executable: &str,
    node_config_paths: &[PathBuf],
    node_runner: &NodeRunner,
    restart_backoffs: &[Duration],
    shutdown_token: &CancellationToken,
) -> Option<io::Result<ExitStatus>> {
    let num_attempts = restart_backoffs.len() + 1;
    let mut attempt = 1;
    loop {
        let node_exit = run_node_once(
            node_executable,
            node_config_paths,
            node_runner,
            attempt > 1,
            shutdown_token,
        )
        .await?;
        if !node_exit.failed_to_bind_address_in_use {
            return Some(node_exit.status);
        }
        let Some(backoff) = restart_backoffs.get(attempt - 1) else {
            error!(
                "Node {node_runner:?} failed to bind an address already in use on all \
                 {num_attempts} attempts."
            );
            return Some(node_exit.status);
        };
        warn!(
            "Node {node_runner:?} failed to bind an address already in use on attempt \
             {attempt}/{num_attempts}. Restarting in {backoff:?}."
        );
        // Prefers the cancellation when both are ready, so a shutdown during the backoff starts
        // no other attempt.
        tokio::select! {
            biased;
            () = shutdown_token.cancelled() => return None,
            () = sleep(*backoff) => {}
        }
        attempt += 1;
    }
}

struct NodeExit {
    status: io::Result<ExitStatus>,
    failed_to_bind_address_in_use: bool,
}

/// Returns `None` if `shutdown_token` is cancelled before the node exits, once the node was killed.
async fn run_node_once(
    node_executable: &str,
    node_config_paths: &[PathBuf],
    node_runner: &NodeRunner,
    append_to_log: bool,
    shutdown_token: &CancellationToken,
) -> Option<NodeExit> {
    // Obtain handles, as the processes and tasks are terminated when their handles are dropped.
    let (mut node_process, stdout_task, stderr_task) = spawn_node_child_process(
        node_executable,
        node_config_paths,
        node_runner.clone(),
        append_to_log,
    )
    .await;
    let status = wait_for_exit_or_shutdown(&mut node_process, shutdown_token).await?;
    // Each pipe has its own drain timeout, so a slow stdout drain cannot hide the stderr result. A
    // stderr that does not reach end of file in time counts as no address-in-use report.
    let (_, stderr_drain) = tokio::join!(
        timeout(PIPE_DRAIN_TIMEOUT, stdout_task),
        timeout(PIPE_DRAIN_TIMEOUT, stderr_task)
    );
    Some(NodeExit { status, failed_to_bind_address_in_use: matches!(stderr_drain, Ok(Ok(true))) })
}

/// Returns the node process, the task forwarding its stdout, and the task forwarding its stderr,
/// which returns whether the stderr reported an address already in use.
#[instrument(skip(node_runner))]
async fn spawn_node_child_process(
    node_executable: &str,
    node_config_paths: &[PathBuf],
    node_runner: NodeRunner,
    append_to_log: bool,
) -> (Child, AbortOnDropHandle<()>, AbortOnDropHandle<bool>) {
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
        .stderr(Stdio::piped())
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

    // Get the node stdout and stderr.
    let node_stdout = node_process.stdout.take().expect("Node stdout should be available.");
    let node_stderr = node_process.stderr.take().expect("Node stderr should be available.");

    // Spawn a task to read node stdout and write to both file and synchronized stdout.
    let stdout_task = AbortOnDropHandle::new(tokio::spawn(async move {
        let mut reader = BufReader::new(node_stdout).lines();
        info!("Writing node logs to file: {:?}", node_runner.logs_file_path());
        let mut file = Some(
            OpenOptions::new()
                .create(true)
                .write(true)
                .append(append_to_log)
                .truncate(!append_to_log)
                .open(node_runner.logs_file_path())
                .await
                .expect("Failed to open log file."),
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

    // Spawn a task to forward node stderr unchanged.
    let stderr_task = AbortOnDropHandle::new(tokio::spawn(async move {
        let mut reader = BufReader::new(node_stderr);
        let mut line = Vec::new();
        let mut address_in_use_reported = false;
        loop {
            line.clear();
            match reader.read_until(b'\n', &mut line).await {
                Ok(0) => return address_in_use_reported,
                Ok(_) => {
                    address_in_use_reported |=
                        String::from_utf8_lossy(&line).contains(ADDRESS_IN_USE_MESSAGE);
                    if let Err(write_error) = stderr().write_all(&line) {
                        error!("Failed to forward node stderr: {}", write_error);
                    }
                }
                Err(read_error) => {
                    error!("Error while reading node stderr: {}", read_error);
                    return address_in_use_reported;
                }
            }
        }
    }));

    (node_process, stdout_task, stderr_task)
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
