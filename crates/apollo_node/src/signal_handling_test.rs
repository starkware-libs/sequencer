use nix::sys::signal;
use nix::unistd::getpid;
use rstest::rstest;
use tokio::time::{sleep, Duration};
use tracing_test::traced_test;

use super::handle_signals;

#[traced_test]
#[rstest]
#[case::sigterm(signal::Signal::SIGTERM, "Received SIGTERM")]
#[case::sigint(signal::Signal::SIGINT, "Received SIGINT")]
#[case::sighup(signal::Signal::SIGHUP, "Received SIGHUP")]
#[case::sigabrt(signal::Signal::SIGABRT, "Received SIGABRT")]
#[tokio::test]
async fn test_signal_handling(#[case] sig: signal::Signal, #[case] expected_message: &str) {
    // Spawn the signal handler
    tokio::spawn(handle_signals());

    // Give it a moment to set up
    sleep(Duration::from_millis(10)).await;

    // Send signal to ourselves
    let pid = getpid();
    signal::kill(pid, sig).unwrap_or_else(|_| panic!("Failed to send {:?}", sig));

    // Wait for the signal to be processed and logged
    sleep(Duration::from_millis(100)).await;

    // Verify the log message was written
    // traced_test captures logs, so we can check them using logs_contain
    assert!(
        logs_contain(expected_message),
        "Expected log message '{}' not found in logs",
        expected_message
    );
}
