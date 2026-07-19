//! Saturation tracking for the prover's concurrency-limited request path.
//!
//! Written by `ProvingRpcServerImpl` (on reject/acquire/release) and read by `HealthLayer`, so
//! `/health` can flip to 503 once the service has been rejecting requests for a sustained period.

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
    /// first rejection since the last `mark_accepted` (or since startup).
    pub fn mark_rejected(&self) {
        let mut state = self.state.lock().expect("saturation lock poisoned");
        if state.is_none() {
            *state = Some(Instant::now());
        }
    }

    /// Record a successful acquire. Clears the saturation window.
    pub fn mark_accepted(&self) {
        self.clear_saturation_window();
    }

    /// Record a worker-slot release (proving finished, failed, or client disconnect). Clears the
    /// saturation window so an idle pod recovers even after the 503 has stopped the load balancer's
    /// traffic — a clear that needed new requests would latch 503 forever.
    pub fn mark_slot_released(&self) {
        self.clear_saturation_window();
    }

    fn clear_saturation_window(&self) {
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
