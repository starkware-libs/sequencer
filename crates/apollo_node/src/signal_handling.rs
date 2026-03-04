use std::future::Future;
use std::pin::Pin;

use tokio::signal::unix::{signal, SignalKind};
use tracing::warn;

#[cfg(test)]
#[path = "signal_handling_test.rs"]
mod signal_handling_test;

/// Boxed future type used for per-signal graceful shutdown.
pub type GracefulShutdownFuture = Pin<Box<dyn Future<Output = ()> + Send>>;
pub type GracefulShutdownCallback = Box<dyn Fn() -> GracefulShutdownFuture + Send>;

/// Per-signal futures for graceful shutdown. Each member is a function that returns the future to
/// run when that signal is received.
pub struct GracefulShutdownBehavior {
    sigterm: GracefulShutdownCallback,
    sigint: GracefulShutdownCallback,
    sigabrt: GracefulShutdownCallback,
}

impl GracefulShutdownBehavior {
    /// Creates a new `GracefulShutdownBehavior` with the given callbacks.
    pub fn new() -> Self {
        // Type annotations are needed to prevent type inference issues.
        let sigterm: GracefulShutdownCallback = Box::new(|| Box::pin(async {}));
        let sigint: GracefulShutdownCallback = Box::new(|| Box::pin(async {}));
        let sigabrt: GracefulShutdownCallback = Box::new(|| Box::pin(async {}));
        Self { sigterm, sigint, sigabrt }
    }

    pub fn sigterm(&self) -> &GracefulShutdownCallback {
        &self.sigterm
    }

    pub fn sigint(&self) -> &GracefulShutdownCallback {
        &self.sigint
    }

    pub fn sigabrt(&self) -> &GracefulShutdownCallback {
        &self.sigabrt
    }

    pub fn with_sigterm(mut self, sigterm: GracefulShutdownCallback) -> Self {
        self.sigterm = sigterm;
        self
    }

    pub fn with_sigint(mut self, sigint: GracefulShutdownCallback) -> Self {
        self.sigint = sigint;
        self
    }
    pub fn with_sigabrt(mut self, sigabrt: GracefulShutdownCallback) -> Self {
        self.sigabrt = sigabrt;
        self
    }
}

impl Default for GracefulShutdownBehavior {
    fn default() -> Self {
        Self::new()
    }
}

/// Handles the SIGTERM, SIGINT, and SIGABRT signals. Upon receiving a signal, the function logs the
/// signal, runs the corresponding graceful shutdown future for that signal, and returns.
pub async fn handle_signals(graceful_shutdown: GracefulShutdownBehavior) {
    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to set up SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("Failed to set up SIGINT handler");
    // SIGABRT is signal 6 on Unix systems
    let mut sigabrt = signal(SignalKind::from_raw(6)).expect("Failed to set up SIGABRT handler");

    tokio::select! {
        _ = sigterm.recv() => {
            warn!("Received SIGTERM");
            (graceful_shutdown.sigterm())().await;
        }
        _ = sigint.recv() => {
            warn!("Received SIGINT");
            (graceful_shutdown.sigint())().await;
        }
        _ = sigabrt.recv() => {
            warn!("Received SIGABRT");
            (graceful_shutdown.sigabrt())().await;
        }
    }
}
