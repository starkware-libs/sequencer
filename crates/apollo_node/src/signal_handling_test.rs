use std::sync::OnceLock;

use nix::sys::signal;
use nix::unistd::getpid;
use rstest::rstest;
use tokio::sync::Mutex;
use tokio::task::yield_now;
use tracing_test::traced_test;

use crate::signal_handling::handle_signals;

// Mutex to ensure tests run one at a time
static TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

// This test sends and captures signals, operations handled at the process level. `cargo test` runs
// different test cases concurrently using different threads, which interferes with the signal
// handling. We therefore use a mutex to ensure only a single test case runs per process at a time.
#[traced_test]
#[rstest]
#[case::sigterm(signal::Signal::SIGTERM, "Received SIGTERM")]
#[case::sigint(signal::Signal::SIGINT, "Received SIGINT")]
#[case::sigabrt(signal::Signal::SIGABRT, "Received SIGABRT")]
#[tokio::test]
async fn test_signal_handling(#[case] sig: signal::Signal, #[case] expected_message: &str) {
    // Lock the mutex to ensure only one test runs at a time.
    let _guard = TEST_MUTEX.get_or_init(|| Mutex::new(())).lock().await;

    // Spawn the signal handler.
    let signal_handler_handle = tokio::spawn(handle_signals());

    // Let the signal handler run.
    yield_now().await;

    // Send signal to ourselves.
    let pid = getpid();
    signal::kill(pid, sig).unwrap_or_else(|_| panic!("Failed to send {sig:?}"));

    // Wait for the signal to be processed and logged.
    signal_handler_handle.await.unwrap();

    // Verify the log message was written.
    assert!(
        logs_contain(expected_message),
        "Expected log message '{expected_message}' not found in logs"
    );
}
