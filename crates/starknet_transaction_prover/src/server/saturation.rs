//! Saturation tracking for the prover's concurrency-limited request path.
//!
//! `ProvingRpcServerImpl` records rejects, worker-slot acquisitions and releases here.
//! `saturated_for_at_least` then answers how long the service has been continuously rejecting
//! requests, which separates sustained overload from an isolated reject.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(test)]
#[path = "saturation_test.rs"]
mod saturation_test;

/// Cheap-to-clone handle to the shared saturation state.
#[derive(Clone, Default)]
pub struct SaturationMonitor {
    state: Arc<Mutex<Option<Instant>>>,
}

impl SaturationMonitor {
    /// Record a rejection. Starts the saturation window if this is the
    /// first rejection since the last `mark_progress` (or since startup).
    pub fn mark_rejected(&self) {
        let mut state = self.state.lock().expect("saturation lock poisoned");
        if state.is_none() {
            *state = Some(Instant::now());
        }
    }

    /// Record forward progress. Either a request acquired a worker slot, or a slot
    /// came free because proving finished, proving failed, or the client disconnected.
    /// Either way the service is not stuck rejecting, so the saturation window clears.
    ///
    /// Slot release has to count. If only a fresh acquisition cleared the window, a
    /// service whose traffic dries up after a burst of rejects would keep reporting
    /// saturation with nothing left to reject.
    pub fn mark_progress(&self) {
        let mut state = self.state.lock().expect("saturation lock poisoned");
        *state = None;
    }

    /// Returns true when the service has been continuously rejecting
    /// requests for at least `threshold`. Returns false when the service
    /// has handled at least one request successfully within the window or
    /// has not seen any traffic at all.
    pub fn saturated_for_at_least(&self, threshold: Duration) -> bool {
        self.state
            .lock()
            .expect("saturation lock poisoned")
            .is_some_and(|started_at| started_at.elapsed() >= threshold)
    }
}
