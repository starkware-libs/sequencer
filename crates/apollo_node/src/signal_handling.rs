use tokio::signal::unix::{signal, SignalKind};
use tracing::warn;

#[cfg(test)]
#[path = "signal_handling_test.rs"]
mod signal_handling_test;

/// Handles the SIGTERM, SIGINT, and SIGABRT signals. Upon receiving a signal, the function logs the
/// signal and returns.
pub async fn handle_signals() {
    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to set up SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("Failed to set up SIGINT handler");
    // SIGABRT is signal 6 on Unix systems
    let mut sigabrt = signal(SignalKind::from_raw(6)).expect("Failed to set up SIGABRT handler");

    tokio::select! {
        _ = sigterm.recv() => {
            warn!("Received SIGTERM");
        }
        _ = sigint.recv() => {
            warn!("Received SIGINT");
        }
        _ = sigabrt.recv() => {
            warn!("Received SIGABRT");
        }
    }
}
