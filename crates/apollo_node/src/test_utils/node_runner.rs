use std::fs::create_dir_all;
use std::io::{stdout, Stdout, Write};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use apollo_config::CONFIG_FILE_ARG;
use apollo_infra_utils::command::create_shell_command;
use apollo_infra_utils::path::resolve_project_relative_path;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Child;
use tokio::task;
use tokio::time::sleep;
use tokio_util::task::AbortOnDropHandle;
use tracing::{error, info, instrument, warn};

pub const NODE_EXECUTABLE_PATH: &str = "target/debug/apollo_node";
const TEMP_LOGS_DIR: &str = "integration_test_temporary_logs";

/// Number of times a node that fails to start is respawned before the run is failed.
const MAX_NODE_STARTUP_ATTEMPTS: usize = 3;

/// A node that exits within this window of being spawned is treated as having failed to start.
/// Exiting later is a crash during the test, which stays fatal.
const NODE_STARTUP_WINDOW: Duration = Duration::from_secs(30);

const NODE_RESTART_BACKOFF: Duration = Duration::from_secs(2);

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

/// Runs the node, restarting it if it exits within `NODE_STARTUP_WINDOW` of being spawned.
///
/// A node that dies that early failed to start, and the cause is often transient: a port that
/// passed the allocator's probe can be taken by another process before the node binds it, which
/// surfaces as `Os { code: 98, kind: AddrInUse }` and otherwise fails the whole run. The retry
/// covers any early exit rather than that error alone, since the cause is only visible in the
/// child's own output. A node that keeps failing to start still fails the run, after
/// `MAX_NODE_STARTUP_ATTEMPTS` attempts, and a crash later in the test stays fatal immediately.
pub fn spawn_run_node(
    node_config_paths: Vec<PathBuf>,
    node_runner: NodeRunner,
) -> AbortOnDropHandle<()> {
    AbortOnDropHandle::new(task::spawn(async move {
        let mut attempt = 1;
        loop {
            info!("Running the node from its spawned task.");
            // Obtain handles, as the processes and task are terminated when their handles are
            // dropped.
            let (mut node_handle, _pipe_task) =
                spawn_node_child_process(node_config_paths.clone(), node_runner.clone()).await;
            let spawned_at = Instant::now();
            let exit_status = node_handle.
                wait(). // Runs the node until completion, should be running indefinitely.
                await; // Awaits the completion of the node.
            let uptime = spawned_at.elapsed();

            if uptime >= NODE_STARTUP_WINDOW || attempt == MAX_NODE_STARTUP_ATTEMPTS {
                panic!(
                    "Node {node_runner:?} stopped unexpectedly after {uptime:?}, on startup \
                     attempt {attempt}/{MAX_NODE_STARTUP_ATTEMPTS}. Exit status: {exit_status:?}."
                );
            }

            warn!(
                "Node {node_runner:?} exited after {uptime:?}, within the {NODE_STARTUP_WINDOW:?} \
                 startup window, so it failed to start. Restarting, attempt \
                 {next_attempt}/{MAX_NODE_STARTUP_ATTEMPTS}. Exit status: {exit_status:?}.",
                next_attempt = attempt + 1
            );
            attempt += 1;
            sleep(NODE_RESTART_BACKOFF).await;
        }
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
