use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::{Duration, Instant};
use std::{fs, thread};

use nix::errno::Errno;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use tempfile::TempDir;
use tokio::time::{sleep, timeout};
use tokio_util::sync::CancellationToken;

use super::{
    run_node_until_fatal_exit,
    spawn_run_node_executable,
    NodeRunner,
    NODE_RESTART_BACKOFFS,
};

const STUB_PID_FILE_NAME: &str = "pid";
const STUB_START_TIMEOUT: Duration = Duration::from_secs(10);
const STUB_EXIT_TIMEOUT: Duration = Duration::from_secs(10);
const STUB_POLL_INTERVAL: Duration = Duration::from_millis(10);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const BIND_FAILURE_LINE: &str = "Failed to bind remote component server socket 127.0.0.1:1: \
                                 Address already in use (os error 98)";
const OTHER_FAILURE_LINE: &str = "Failed to load and validate config";
const SHORT_RESTART_BACKOFFS: [Duration; 2] = [Duration::from_millis(10); 2];
const LONG_RESTART_BACKOFFS: [Duration; 1] = [Duration::from_secs(60)];
const RUNNING_NODE_OBSERVATION_TIME: Duration = Duration::from_secs(5);
const STUB_RUNS_FILE_NAME: &str = "runs";
const NUM_RUNS_ALWAYS_FAILING: usize = 1000;
/// Starts a background process that holds the stub's stdout open for longer than
/// `PIPE_DRAIN_TIMEOUT` after the stub exits, and leaves its stderr unheld.
const HOLD_STDOUT_OPEN_COMMAND: &str = "sleep 3 2>/dev/null </dev/null &";
/// Deletes the running stub, so a later spawn of it panics the node task.
const DELETE_STUB_COMMAND: &str = "rm -- \"$0\"";

/// Writes `script` into `stub_dir` as an executable stub node and returns its path.
fn write_executable_stub(stub_dir: &Path, script: &str) -> String {
    let stub_path = stub_dir.join("stub_node.sh");
    fs::write(&stub_path, script).unwrap();
    fs::set_permissions(&stub_path, fs::Permissions::from_mode(0o755)).unwrap();
    stub_path.to_string_lossy().to_string()
}

/// Returns a shell command that records the stub's PID in `stub_dir`. The rename publishes the PID
/// file only once it is complete.
fn record_pid_command(stub_dir: &Path) -> String {
    format!(
        "echo $$ > '{pid_file}.tmp' && mv '{pid_file}.tmp' '{pid_file}'",
        pid_file = stub_dir.join(STUB_PID_FILE_NAME).display(),
    )
}

/// Writes a stub node into `stub_dir` that records its PID and keeps running.
fn write_pid_recording_stub_node(stub_dir: &Path) -> String {
    let script = format!("#!/bin/sh\n{}\nexec sleep 60\n", record_pid_command(stub_dir));
    write_executable_stub(stub_dir, &script)
}

async fn await_stub_pid(stub_dir: &Path) -> Pid {
    let pid_file = stub_dir.join(STUB_PID_FILE_NAME);
    timeout(STUB_START_TIMEOUT, async {
        loop {
            if let Ok(pid) = fs::read_to_string(&pid_file) {
                return Pid::from_raw(pid.trim().parse().unwrap());
            }
            sleep(STUB_POLL_INTERVAL).await;
        }
    })
    .await
    .expect("Stub node should record its PID.")
}

/// Blocks the thread until the process `pid` exited but is not yet reaped.
fn block_until_unreaped_exit(pid: Pid) {
    let deadline = Instant::now() + STUB_EXIT_TIMEOUT;
    loop {
        let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
        // The state follows the parenthesized command name, and `Z` means exited but not reaped.
        if stat.rsplit_once(") ").unwrap().1.starts_with('Z') {
            return;
        }
        assert!(Instant::now() < deadline, "Process {pid} should have exited.");
        thread::sleep(STUB_POLL_INTERVAL);
    }
}

fn is_reaped(pid: Pid) -> bool {
    // Signal 0 only checks whether the process exists, which it does until it is reaped.
    kill(pid, None) == Err(nix::Error::Sys(Errno::ESRCH))
}

/// Waits until the process `pid` is reaped, which `Child::wait` does once it observes the exit.
async fn await_reaped(pid: Pid) {
    timeout(STUB_EXIT_TIMEOUT, async {
        while !is_reaped(pid) {
            sleep(STUB_POLL_INTERVAL).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("Process {pid} should be reaped."));
}

/// Writes a stub node into `stub_dir` that counts its runs. Each of its first `num_failing_runs`
/// runs prints `stderr_line` to stderr and exits with `exit_code`; later runs keep running.
fn write_stub_node(
    stub_dir: &Path,
    num_failing_runs: usize,
    stderr_line: &str,
    exit_code: i32,
) -> String {
    write_stub_node_with_failing_run_command(
        stub_dir,
        num_failing_runs,
        stderr_line,
        exit_code,
        ":",
    )
}

/// Like `write_stub_node`, where each failing run first runs the shell command
/// `failing_run_command`.
fn write_stub_node_with_failing_run_command(
    stub_dir: &Path,
    num_failing_runs: usize,
    stderr_line: &str,
    exit_code: i32,
    failing_run_command: &str,
) -> String {
    let runs_file = stub_dir.join(STUB_RUNS_FILE_NAME);
    let script = format!(
        "#!/bin/sh\nrun=$(( $(cat '{runs}' 2>/dev/null || echo 0) + 1 ))\necho \"$run\" > \
         '{runs}'\nif [ \"$run\" -le {num_failing_runs} ]; then\n  {failing_run_command}\n  echo \
         '{stderr_line}' >&2\n  exit {exit_code}\nfi\nexec sleep 60\n",
        runs = runs_file.display(),
    );
    write_executable_stub(stub_dir, &script)
}

fn num_stub_runs(stub_dir: &Path) -> usize {
    fs::read_to_string(stub_dir.join(STUB_RUNS_FILE_NAME)).unwrap().trim().parse().unwrap()
}

#[tokio::test]
async fn shutdown_returns_after_the_node_process_exited() {
    let stub_dir = TempDir::new().unwrap();
    let stub_node = write_pid_recording_stub_node(stub_dir.path());
    let node_process_handle = spawn_run_node_executable(
        stub_node,
        vec![],
        NodeRunner::new(0, "stub_shutdown".to_string()),
        &NODE_RESTART_BACKOFFS,
    );
    let node_pid = await_stub_pid(stub_dir.path()).await;

    timeout(SHUTDOWN_TIMEOUT, node_process_handle.shutdown()).await.unwrap().unwrap();

    assert!(is_reaped(node_pid), "Node process {node_pid} should have exited.");
}

#[tokio::test]
async fn shutdown_after_the_node_stopped_returns_the_task_panic() {
    let stub_dir = TempDir::new().unwrap();
    let stub_node = write_executable_stub(stub_dir.path(), "#!/bin/sh\nexit 1\n");
    let node_process_handle = spawn_run_node_executable(
        stub_node,
        vec![],
        NodeRunner::new(0, "stub_stopped".to_string()),
        &NODE_RESTART_BACKOFFS,
    );
    timeout(STUB_EXIT_TIMEOUT, async {
        while !node_process_handle.is_finished() {
            sleep(STUB_POLL_INTERVAL).await;
        }
    })
    .await
    .expect("Node task should finish once the stub node exits.");

    let shutdown_error =
        timeout(SHUTDOWN_TIMEOUT, node_process_handle.shutdown()).await.unwrap().unwrap_err();

    assert!(shutdown_error.is_panic(), "Expected the node task to panic, got {shutdown_error:?}.");
}

// The current-thread runtime cannot poll the node task while the test blocks, so the task is next
// polled with both the node exit and the shutdown pending.
#[tokio::test(flavor = "current_thread")]
async fn shutdown_after_an_unobserved_node_exit_returns_the_task_panic() {
    let stub_dir = TempDir::new().unwrap();
    let stub_node = write_pid_recording_stub_node(stub_dir.path());
    let node_process_handle = spawn_run_node_executable(
        stub_node,
        vec![],
        NodeRunner::new(0, "stub_unobserved_exit".to_string()),
        &NODE_RESTART_BACKOFFS,
    );
    let node_pid = await_stub_pid(stub_dir.path()).await;
    kill(node_pid, Signal::SIGKILL).unwrap();
    block_until_unreaped_exit(node_pid);

    let shutdown_error =
        timeout(SHUTDOWN_TIMEOUT, node_process_handle.shutdown()).await.unwrap().unwrap_err();

    assert!(shutdown_error.is_panic(), "Expected the node task to panic, got {shutdown_error:?}.");
}

#[tokio::test]
async fn bind_failure_is_restarted_until_the_node_stays_up() {
    let stub_dir = TempDir::new().unwrap();
    let stub_node = write_stub_node(stub_dir.path(), 2, BIND_FAILURE_LINE, 1);
    let node_runner = NodeRunner::new(0, "stub_bind_failure_then_up".to_string());

    let run_result = timeout(
        RUNNING_NODE_OBSERVATION_TIME,
        run_node_until_fatal_exit(
            &stub_node,
            &[],
            &node_runner,
            &SHORT_RESTART_BACKOFFS,
            &CancellationToken::new(),
        ),
    )
    .await;

    assert!(run_result.is_err(), "Node should still be running, got {run_result:?}.");
    assert_eq!(num_stub_runs(stub_dir.path()), 3);
}

#[tokio::test]
async fn bind_failure_is_restarted_while_stdout_is_still_open() {
    let stub_dir = TempDir::new().unwrap();
    let stub_node = write_stub_node_with_failing_run_command(
        stub_dir.path(),
        1,
        BIND_FAILURE_LINE,
        1,
        HOLD_STDOUT_OPEN_COMMAND,
    );
    let node_runner = NodeRunner::new(0, "stub_bind_failure_stdout_open".to_string());

    let run_result = timeout(
        RUNNING_NODE_OBSERVATION_TIME,
        run_node_until_fatal_exit(
            &stub_node,
            &[],
            &node_runner,
            &SHORT_RESTART_BACKOFFS,
            &CancellationToken::new(),
        ),
    )
    .await;

    assert!(run_result.is_err(), "Node should still be running, got {run_result:?}.");
    assert_eq!(num_stub_runs(stub_dir.path()), 2);
}

#[tokio::test]
async fn bind_failure_on_every_attempt_returns_the_last_exit() {
    let stub_dir = TempDir::new().unwrap();
    let stub_node = write_stub_node(stub_dir.path(), NUM_RUNS_ALWAYS_FAILING, BIND_FAILURE_LINE, 1);
    let node_runner = NodeRunner::new(0, "stub_bind_failure_exhausted".to_string());

    let exit_status = run_node_until_fatal_exit(
        &stub_node,
        &[],
        &node_runner,
        &SHORT_RESTART_BACKOFFS,
        &CancellationToken::new(),
    )
    .await
    .unwrap()
    .unwrap();

    assert_eq!(exit_status.code(), Some(1));
    assert_eq!(num_stub_runs(stub_dir.path()), SHORT_RESTART_BACKOFFS.len() + 1);
}

#[tokio::test]
async fn other_exit_returns_at_once_without_restart() {
    let stub_dir = TempDir::new().unwrap();
    let stub_node =
        write_stub_node(stub_dir.path(), NUM_RUNS_ALWAYS_FAILING, OTHER_FAILURE_LINE, 2);
    let node_runner = NodeRunner::new(0, "stub_other_failure".to_string());

    let exit_status = timeout(
        RUNNING_NODE_OBSERVATION_TIME,
        run_node_until_fatal_exit(
            &stub_node,
            &[],
            &node_runner,
            &LONG_RESTART_BACKOFFS,
            &CancellationToken::new(),
        ),
    )
    .await
    .expect("A non-bind exit should return without waiting for a restart.")
    .unwrap()
    .unwrap();

    assert_eq!(exit_status.code(), Some(2));
    assert_eq!(num_stub_runs(stub_dir.path()), 1);
}

#[tokio::test]
async fn shutdown_during_the_restart_backoff_returns_without_a_restart() {
    let stub_dir = TempDir::new().unwrap();
    let stub_node = write_stub_node_with_failing_run_command(
        stub_dir.path(),
        1,
        BIND_FAILURE_LINE,
        1,
        &format!("{}; {DELETE_STUB_COMMAND}", record_pid_command(stub_dir.path())),
    );
    let node_process_handle = spawn_run_node_executable(
        stub_node,
        vec![],
        NodeRunner::new(0, "stub_shutdown_during_backoff".to_string()),
        &LONG_RESTART_BACKOFFS,
    );
    // The node task enters the backoff once it reaps the first run.
    await_reaped(await_stub_pid(stub_dir.path()).await).await;

    // `SHUTDOWN_TIMEOUT` is shorter than the backoff, so the shutdown must interrupt it.
    let shutdown_result = timeout(SHUTDOWN_TIMEOUT, node_process_handle.shutdown()).await.unwrap();

    assert!(
        shutdown_result.is_ok(),
        "Expected no restart after the shutdown, got {shutdown_result:?}."
    );
}
