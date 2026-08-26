use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use tempfile::TempDir;
use tokio::time::timeout;

use super::{run_node_until_fatal_exit, NodeRunner};

const BIND_FAILURE_LINE: &str = "Failed to bind remote component server socket 127.0.0.1:1: \
                                 Address already in use (os error 98)";
const OTHER_FAILURE_LINE: &str = "Failed to load and validate config";
const SHORT_RESTART_BACKOFFS: [Duration; 2] = [Duration::from_millis(10); 2];
const RUNNING_NODE_OBSERVATION_TIME: Duration = Duration::from_secs(5);
const STUB_RUNS_FILE_NAME: &str = "runs";
const NUM_RUNS_ALWAYS_FAILING: usize = 1000;

/// Writes a stub node into `stub_dir` that counts its runs. Each of its first `num_failing_runs`
/// runs prints `stderr_line` to stderr and exits with `exit_code`; later runs keep running.
fn write_stub_node(
    stub_dir: &Path,
    num_failing_runs: usize,
    stderr_line: &str,
    exit_code: i32,
) -> String {
    let runs_file = stub_dir.join(STUB_RUNS_FILE_NAME);
    let script = format!(
        "#!/bin/sh\nrun=$(( $(cat '{runs}' 2>/dev/null || echo 0) + 1 ))\necho \"$run\" > \
         '{runs}'\nif [ \"$run\" -le {num_failing_runs} ]; then\n  echo '{stderr_line}' >&2\n  \
         exit {exit_code}\nfi\nexec sleep 60\n",
        runs = runs_file.display(),
    );
    let stub_path = stub_dir.join("stub_node.sh");
    fs::write(&stub_path, script).unwrap();
    fs::set_permissions(&stub_path, fs::Permissions::from_mode(0o755)).unwrap();
    stub_path.to_string_lossy().to_string()
}

fn num_stub_runs(stub_dir: &Path) -> usize {
    fs::read_to_string(stub_dir.join(STUB_RUNS_FILE_NAME)).unwrap().trim().parse().unwrap()
}

#[tokio::test]
async fn bind_failure_is_restarted_until_the_node_stays_up() {
    let stub_dir = TempDir::new().unwrap();
    let stub_node = write_stub_node(stub_dir.path(), 2, BIND_FAILURE_LINE, 1);
    let node_runner = NodeRunner::new(0, "stub_bind_failure_then_up".to_string());

    let run_result = timeout(
        RUNNING_NODE_OBSERVATION_TIME,
        run_node_until_fatal_exit(&stub_node, &[], &node_runner, &SHORT_RESTART_BACKOFFS),
    )
    .await;

    assert!(run_result.is_err(), "Node should still be running, got {run_result:?}.");
    assert_eq!(num_stub_runs(stub_dir.path()), 3);
}

#[tokio::test]
async fn bind_failure_on_every_attempt_returns_the_last_exit() {
    let stub_dir = TempDir::new().unwrap();
    let stub_node = write_stub_node(stub_dir.path(), NUM_RUNS_ALWAYS_FAILING, BIND_FAILURE_LINE, 1);
    let node_runner = NodeRunner::new(0, "stub_bind_failure_exhausted".to_string());

    let exit_status =
        run_node_until_fatal_exit(&stub_node, &[], &node_runner, &SHORT_RESTART_BACKOFFS)
            .await
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
    let long_restart_backoffs = [Duration::from_secs(60)];

    let exit_status = timeout(
        RUNNING_NODE_OBSERVATION_TIME,
        run_node_until_fatal_exit(&stub_node, &[], &node_runner, &long_restart_backoffs),
    )
    .await
    .expect("A non-bind exit should return without waiting for a restart.")
    .unwrap();

    assert_eq!(exit_status.code(), Some(2));
    assert_eq!(num_stub_runs(stub_dir.path()), 1);
}
