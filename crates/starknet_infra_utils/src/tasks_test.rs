use core::panic;
use std::future::pending;

use rstest::rstest;
use tokio::signal::unix::{signal, SignalKind};
use tokio::time::{sleep, timeout, Duration};

use crate::tasks::{inner_spawn_with_exit_on_panic, spawn_protected, spawn_with_exit_on_panic};

const SUCCESS_VALUE: u32 = 5;

fn panicking_fn() -> u32 {
    panic!("Oh no!");
}

fn success_fn() -> u32 {
    SUCCESS_VALUE
}

async fn never_ending_fn() -> u32 {
    pending::<u32>().await
}

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

#[rstest]
#[tokio::test]
async fn spawn_protected_success_with_handle() {
    let handle = spawn_protected(async { success_fn() });
    let result = handle.await.unwrap();
    assert_eq!(result, SUCCESS_VALUE);
}

#[rstest]
#[tokio::test]
async fn spawn_protected_success_without_handle() {
    let result = spawn_protected(async { success_fn() }).await.unwrap();
    assert_eq!(result, SUCCESS_VALUE);
}

#[rstest]
#[tokio::test]
#[should_panic(expected = "Unresolved ProtectedJoinHandle dropped")]
async fn spawn_protected_unresolved() {
    spawn_protected(async { success_fn() });
}

#[rstest]
#[tokio::test]
#[should_panic(expected = "Oh no!")]
async fn spawn_protected_single_panic() {
    spawn_protected(async { panicking_fn() }).await.unwrap();
}

// The expected panic in the 2-level-nesting tested case is:
// 'JoinError::Panic(Id(X), "JoinError::Panic(Id(Y), "Oh no!", ...)", ...)', where 'X' and 'Y'
// are the non-deterministic tokio inner task identifiers. The 'should_panic' annotation does not
// support pattern matching, and as such, the test expected the suffix '// "Oh no!", ...)", ...)'
// that captures the original panic message while indicating the nesting level.
#[rstest]
#[tokio::test]
#[should_panic(expected = "\\\"Oh no!\\\", ...)\", ...)")]
async fn spawn_protected_nested_panic() {
    spawn_protected(async { spawn_protected(async { panicking_fn() }).await.unwrap() })
        .await
        .unwrap();
}

#[rstest]
#[tokio::test]
async fn spawn_protected_abort_panic() {
    spawn_protected(async { panicking_fn() }).abort();
}

#[rstest]
#[tokio::test]
async fn spawn_protected_abort_pending() {
    spawn_protected(never_ending_fn()).abort();
}
