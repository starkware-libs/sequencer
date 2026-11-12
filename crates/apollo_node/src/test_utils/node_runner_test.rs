use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::ExitStatus;
#[cfg(target_os = "linux")]
use std::thread;
use std::time::Duration;
#[cfg(target_os = "linux")]
use std::time::Instant;
use std::{fs, io};

use nix::errno::Errno;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use rstest::rstest;
use tempfile::TempDir;
use tokio::time::{sleep, timeout};

use super::{spawn_run_node_executable, unexpected_exit_after_kill, NodeRunner, SIGKILL};

const STUB_PID_FILE_NAME: &str = "pid";
const STUB_START_TIMEOUT: Duration = Duration::from_secs(10);
const STUB_EXIT_TIMEOUT: Duration = Duration::from_secs(10);
const STUB_POLL_INTERVAL: Duration = Duration::from_millis(10);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

/// Writes `script` into `stub_dir` as an executable stub node and returns its path.
fn write_executable_stub(stub_dir: &Path, script: &str) -> String {
    let stub_path = stub_dir.join("stub_node.sh");
    fs::write(&stub_path, script).unwrap();
    fs::set_permissions(&stub_path, fs::Permissions::from_mode(0o755)).unwrap();
    stub_path.to_string_lossy().to_string()
}

/// Writes a stub node into `stub_dir` that records its PID and keeps running.
fn write_pid_recording_stub_node(stub_dir: &Path) -> String {
    // The rename publishes the PID file only once it is complete.
    let script = format!(
        "#!/bin/sh\necho $$ > '{pid_file}.tmp'\nmv '{pid_file}.tmp' '{pid_file}'\nexec sleep 60\n",
        pid_file = stub_dir.join(STUB_PID_FILE_NAME).display(),
    );
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
#[cfg(target_os = "linux")]
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

#[tokio::test]
async fn shutdown_returns_after_the_node_process_exited() {
    let stub_dir = TempDir::new().unwrap();
    let stub_node = write_pid_recording_stub_node(stub_dir.path());
    let node_process_handle = spawn_run_node_executable(
        stub_node,
        vec![],
        NodeRunner::new(0, "stub_shutdown".to_string()),
    );
    let node_pid = await_stub_pid(stub_dir.path()).await;

    timeout(SHUTDOWN_TIMEOUT, node_process_handle.shutdown()).await.unwrap().unwrap();

    // Signal 0 only checks whether the process exists, which it does until it is reaped.
    assert_eq!(
        kill(node_pid, None),
        Err(nix::Error::Sys(Errno::ESRCH)),
        "Node process {node_pid} should have exited."
    );
}

#[tokio::test]
async fn shutdown_after_the_node_stopped_returns_the_task_panic() {
    let stub_dir = TempDir::new().unwrap();
    let stub_node = write_executable_stub(stub_dir.path(), "#!/bin/sh\nexit 1\n");
    let node_process_handle = spawn_run_node_executable(
        stub_node,
        vec![],
        NodeRunner::new(0, "stub_stopped".to_string()),
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
#[cfg(target_os = "linux")]
#[tokio::test(flavor = "current_thread")]
async fn shutdown_after_an_unobserved_node_exit_returns_the_task_panic() {
    let stub_dir = TempDir::new().unwrap();
    let stub_node = write_pid_recording_stub_node(stub_dir.path());
    let node_process_handle = spawn_run_node_executable(
        stub_node,
        vec![],
        NodeRunner::new(0, "stub_unobserved_exit".to_string()),
    );
    let node_pid = await_stub_pid(stub_dir.path()).await;
    kill(node_pid, Signal::SIGKILL).unwrap();
    block_until_unreaped_exit(node_pid);

    let shutdown_error =
        timeout(SHUTDOWN_TIMEOUT, node_process_handle.shutdown()).await.unwrap().unwrap_err();

    assert!(shutdown_error.is_panic(), "Expected the node task to panic, got {shutdown_error:?}.");
}

#[test]
fn sigkill_exit_after_kill_is_a_shutdown() {
    assert_eq!(Signal::try_from(SIGKILL), Ok(Signal::SIGKILL));

    assert!(unexpected_exit_after_kill(Ok(ExitStatus::from_raw(SIGKILL))).is_none());
}

// A raw wait status holds a signal number in its low 7 bits and an exit code in bits 8 to 15.
#[rstest]
#[case::exit_code_1(ExitStatus::from_raw(1 << 8))]
#[case::sigterm(ExitStatus::from_raw(15))]
fn other_exit_after_kill_is_unexpected(#[case] exit_status: ExitStatus) {
    let unexpected_exit_status = unexpected_exit_after_kill(Ok(exit_status));

    assert_eq!(unexpected_exit_status.unwrap().unwrap(), exit_status);
}

#[test]
fn wait_error_after_kill_is_unexpected() {
    let unexpected_exit_status = unexpected_exit_after_kill(Err(io::Error::other("wait failed")));

    assert!(
        matches!(unexpected_exit_status, Some(Err(_))),
        "Expected the wait error, got {unexpected_exit_status:?}."
    );
}
