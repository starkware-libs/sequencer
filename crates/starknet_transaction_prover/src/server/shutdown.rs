//! Bridges SIGTERM/SIGINT into a graceful jsonrpsee server shutdown.
//!
//! First signal: `ServerHandle::stop`, letting in-flight requests drain.
//! Second signal while the drain is still in progress: `std::process::exit(1)`,
//! so an operator can always reclaim a stuck process. Tokio keeps intercepting
//! the signals process-wide once a handler existed (tokio-rs/tokio#7905), so
//! without the second-signal path a follow-up Ctrl+C would be silently
//! swallowed.

use std::future::Future;

use anyhow::Context;
use jsonrpsee::server::ServerHandle;
use tokio::signal::unix::{signal, Signal, SignalKind};
use tracing::{info, warn};

#[cfg(test)]
#[path = "shutdown_test.rs"]
mod shutdown_test;

/// Spawns the task bridging termination signals into `server_handle.stop()`.
///
/// Fails rather than degrading: a prover that cannot receive `SIGTERM` is killed
/// outright by the scheduler at the end of its grace period, losing an in-flight
/// proof and skipping the drain entirely. Refusing to start makes that a visible
/// deploy failure instead of a surprise on the first rollout.
pub fn spawn_signal_bridge(server_handle: ServerHandle) -> anyhow::Result<()> {
    let sigterm = signal(SignalKind::terminate()).context("Failed to install SIGTERM handler")?;
    let sigint = signal(SignalKind::interrupt()).context("Failed to install SIGINT handler")?;
    tokio::spawn(stop_on_termination_signal(server_handle, sigterm, sigint));
    Ok(())
}

async fn stop_on_termination_signal(
    server_handle: ServerHandle,
    mut sigterm: Signal,
    mut sigint: Signal,
) {
    let signal_name = recv_termination_signal(&mut sigterm, &mut sigint).await;
    info!(event = "shutdown_started", signal = signal_name, "Shutting down JSON-RPC server.");
    if let Err(err) = server_handle.stop() {
        warn!(event = "stop_failed", error = %err, "Failed to stop JSON-RPC server cleanly");
    }

    let outcome = race_shutdown_against_second_signal(
        server_handle.stopped(),
        recv_termination_signal(&mut sigterm, &mut sigint),
    )
    .await;
    if let ShutdownOutcome::ForceExit(second_signal_name) = outcome {
        warn!(
            event = "force_exit",
            signal = second_signal_name,
            "Received second termination signal; forcing exit."
        );
        std::process::exit(1);
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ShutdownOutcome {
    /// The drain finished; a late signal must not flip the exit code.
    Clean,
    /// Carries the name of the second signal.
    ForceExit(&'static str),
}

/// Races a clean shutdown against a second termination signal. `biased` is
/// load-bearing: without it, `tokio::select!` picks a ready branch at random,
/// so a second signal landing in the same poll as `stopped` resolving could
/// force-exit a server that already shut down cleanly. Listing `stopped`
/// first makes it win whenever both are ready.
async fn race_shutdown_against_second_signal(
    stopped: impl Future<Output = ()>,
    second_signal: impl Future<Output = &'static str>,
) -> ShutdownOutcome {
    tokio::select! {
        biased;
        _ = stopped => ShutdownOutcome::Clean,
        second_signal_name = second_signal => ShutdownOutcome::ForceExit(second_signal_name),
    }
}

/// Borrows the handlers, so a second call waits for a follow-up signal.
async fn recv_termination_signal(sigterm: &mut Signal, sigint: &mut Signal) -> &'static str {
    tokio::select! {
        _ = sigterm.recv() => "SIGTERM",
        _ = sigint.recv() => "SIGINT",
    }
}
