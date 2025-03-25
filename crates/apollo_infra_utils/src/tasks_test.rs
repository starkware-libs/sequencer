use rstest::rstest;
use tokio::signal::unix::{signal, SignalKind};
use tokio::time::{sleep, timeout, Duration};

use crate::tasks::{inner_spawn_with_exit_on_panic, spawn_with_exit_on_panic};

#[rstest]
#[tokio::test]
async fn test_spawn_with_exit_on_panic_success() {
    let handle = spawn_with_exit_on_panic(async {
        sleep(Duration::from_millis(10)).await;
    });

    // Await the monitoring task
    handle.await.unwrap();
}

#[rstest]
#[tokio::test]
async fn test_spawn_with_exit_on_panic_failure() {
    // Mock exit process function: instead of calling `std::process::exit(1)`, send 'SIGTERM' to
    // self.
    let mock_exit_process = || {
        // Use fully-qualified nix modules to avoid ambiguity with the tokio ones.
        let pid = nix::unistd::getpid();
        nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM)
            .expect("Failed to send signal");
    };

    // Set up a SIGTERM handler.
    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to set up SIGTERM handler");

    // Spawn a task that panics, and uses the SIGTERM mocked exit process function.
    inner_spawn_with_exit_on_panic(
        async {
            panic!("This task will fail!");
        },
        mock_exit_process,
    );

    // Assert the task failure is detected and that the mocked exit process function is called by
    // awaiting for the SIGTERM signal. Bound the timeout to ensure the test does not hang
    // indefinitely.
    assert!(
        timeout(Duration::from_millis(10), sigterm.recv()).await.is_ok(),
        "Did not receive SIGTERM signal."
    );
}
