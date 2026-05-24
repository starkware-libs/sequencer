//! Saturation tracking for the prover's concurrency-limited request path.
//!
//! Used by both `ProvingRpcServerImpl` (writer) and `HealthLayer` (reader)
//! so `/health` can flip to 503 once the service has been rejecting
//! requests for a sustained period.
//!
//! Mechanism:
//! - On every rejection from the concurrency semaphore: if the monitor is currently "clear", set a
//!   timestamp marking when saturation started.
//! - On every successful acquire: clear the timestamp.
//! - On every worker-slot release (proving finished, failed, or the client disconnected): clear the
//!   timestamp — freed capacity means the service can accept work again.
//! - `/health` consults [`SaturationMonitor::saturated_for_at_least`] with the configured threshold
//!   and returns 503 if it passed.
//!
//! `/health` is therefore 503 only while rejections have been ongoing for the
//! threshold with no worker slot freed and no request accepted in that window.
//! Saturation can only latch while every worker slot is held, so once those
//! in-flight jobs drain the window clears on its own — no new traffic needed.
//! That matters because the 503 itself stops the load balancer from sending
//! traffic; without the release-side clear the monitor would latch 503 forever
//! on an idle pod. This mirrors the proof-interceptor's upstream-reachability
//! tracking (healthy until sustained failures cross a threshold) so the two
//! services behave the same way from a load-balancer's perspective.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(test)]
#[path = "saturation_test.rs"]
mod saturation_test;

/// Cheap-to-clone handle to the shared saturation state. The interior
/// `Arc<Mutex<...>>` makes both the `mark_*` write paths and the read
/// path lock-free at the API surface.
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

    /// Record a successful acquire. Clears the saturation window so a
    /// transient burst of rejections doesn't keep `/health` red forever
    /// once the service recovers.
    pub fn mark_accepted(&self) {
        self.clear_saturation_window();
    }

    /// Record a worker-slot release (proving finished, failed, or the client
    /// disconnected). Clears the saturation window: freed capacity means the
    /// service can accept work again, even if the load balancer has stopped
    /// sending traffic.
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
