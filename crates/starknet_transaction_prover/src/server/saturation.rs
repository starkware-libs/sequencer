//! Saturation tracking for the prover's concurrency-limited request path.
//!
//! `ProvingRpcServerImpl` records rejects, worker-slot acquisitions and releases here. The first
//! reject since the last worker-slot acquisition or release opens a window, and either kind of
//! progress closes it. `saturated_for_at_least` answers whether the current window has been open
//! for a threshold while proving work still holds a worker slot, which separates sustained
//! overload from an isolated reject and keeps an idle service healthy.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

#[cfg(test)]
#[path = "saturation_test.rs"]
mod saturation_test;

/// Cheap-to-clone handle to the shared saturation state.
#[derive(Clone, Default)]
pub struct SaturationMonitor {
    state: Arc<Mutex<SaturationState>>,
}

#[derive(Default)]
struct SaturationState {
    /// When the first reject since the last worker-slot acquisition or release happened. `None`
    /// while no window is open.
    window_opened_at: Option<Instant>,
    /// Worker slots currently held by proving runs.
    num_held_worker_slots: usize,
}

impl SaturationMonitor {
    /// Record a rejection. Starts the saturation window if this is the first rejection since the
    /// last worker-slot acquisition or release (or since startup).
    pub fn mark_rejected(&self) {
        let mut state = self.lock_state();
        if state.window_opened_at.is_none() {
            state.window_opened_at = Some(Instant::now());
        }
    }

    /// Record that a request acquired a worker slot. The service is admitting work, so the
    /// saturation window closes.
    pub fn mark_slot_acquired(&self) {
        let mut state = self.lock_state();
        state.num_held_worker_slots += 1;
        state.window_opened_at = None;
    }

    /// Record that a worker slot came free because proving finished, proving failed, or the
    /// client disconnected. The saturation window closes here too.
    ///
    /// Slot release has to count. If only a fresh acquisition cleared the window, a service whose
    /// traffic dries up after a burst of rejects would keep reporting saturation with nothing left
    /// to reject.
    pub fn mark_slot_released(&self) {
        let mut state = self.lock_state();
        state.num_held_worker_slots = state.num_held_worker_slots.saturating_sub(1);
        state.window_opened_at = None;
    }

    /// Returns true when a rejection window has been open for at least `threshold` and a worker
    /// slot is still held: a request was rejected that long ago, no worker slot has been acquired
    /// or released since, and proving work is still running.
    ///
    /// Returns false on an idle service, even with a window open. A request that found the
    /// service full can record its reject after the last proving run released its slot. That
    /// reject reopens the window, and with no run left to release a slot, nothing would close it
    /// until new traffic arrives.
    pub fn saturated_for_at_least(&self, threshold: Duration) -> bool {
        let state = self.lock_state();
        state.num_held_worker_slots > 0
            && state.window_opened_at.is_some_and(|opened_at| opened_at.elapsed() >= threshold)
    }

    fn lock_state(&self) -> MutexGuard<'_, SaturationState> {
        self.state.lock().expect("saturation lock poisoned")
    }
}
