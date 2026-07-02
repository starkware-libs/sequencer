//! Bridges SIGTERM/SIGINT into a graceful jsonrpsee server shutdown.
//!
//! First signal: `ServerHandle::stop`, letting in-flight requests drain.
//! Second signal while the drain is still in progress: `std::process::exit(1)`,
//! so an operator can always reclaim a stuck process. Tokio keeps intercepting
//! the signals process-wide once a handler existed (tokio-rs/tokio#7905), so
//! without the second-signal path a follow-up Ctrl+C would be silently
//! swallowed. Once the server has stopped cleanly, a late signal no longer
//! force-exits.

use jsonrpsee::server::ServerHandle;
use tokio::signal::unix::{signal, Signal, SignalKind};
use tracing::{info, warn};

/// Spawns the task bridging termination signals into `server_handle.stop()`.
/// Both handlers are installed eagerly: if one fails, the other still drives
/// a graceful shutdown.
pub fn spawn_signal_bridge(server_handle: ServerHandle) {
    let sigterm = signal(SignalKind::terminate())
        .inspect_err(|err| warn!(error = %err, "Failed to install SIGTERM handler"))
        .ok();
    let sigint = signal(SignalKind::interrupt())
        .inspect_err(|err| warn!(error = %err, "Failed to install SIGINT handler"))
        .ok();
    tokio::spawn(stop_on_termination_signal(server_handle, sigterm, sigint));
}

async fn stop_on_termination_signal(
    server_handle: ServerHandle,
    mut sigterm: Option<Signal>,
    mut sigint: Option<Signal>,
) {
    let Some(signal_name) = recv_termination_signal(&mut sigterm, &mut sigint).await else {
        return;
    };
    info!(event = "shutdown_started", signal = signal_name, "Shutting down JSON-RPC server.");
    if let Err(err) = server_handle.stop() {
        warn!(error = %err, "Failed to stop JSON-RPC server cleanly");
    }

    // Force-exit only while the drain is still in progress: once `stopped`
    // resolves the shutdown was clean, and a late signal must not flip the
    // exit code to 1.
    tokio::select! {
        _ = server_handle.stopped() => {}
        second_signal = recv_termination_signal(&mut sigterm, &mut sigint) => {
            if second_signal.is_some() {
                warn!(event = "force_exit", "Received second termination signal; forcing exit.");
                std::process::exit(1);
            }
        }
    }
}

/// Waits for the first termination signal and returns its name; `None` if
/// neither handler is installed. Borrows the handlers, so a second call waits
/// for a follow-up signal.
async fn recv_termination_signal(
    sigterm: &mut Option<Signal>,
    sigint: &mut Option<Signal>,
) -> Option<&'static str> {
    match (sigterm, sigint) {
        (Some(sigterm), Some(sigint)) => tokio::select! {
            _ = sigterm.recv() => Some("SIGTERM"),
            _ = sigint.recv() => Some("SIGINT"),
        },
        (Some(sigterm), None) => {
            sigterm.recv().await;
            Some("SIGTERM")
        }
        (None, Some(sigint)) => {
            sigint.recv().await;
            Some("SIGINT")
        }
        (None, None) => None,
    }
}
