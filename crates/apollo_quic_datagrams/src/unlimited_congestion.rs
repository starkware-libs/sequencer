// Copyright 2024 Starkware
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

//! Unlimited congestion controller that behaves like UDP.
//!
//! This controller provides NO congestion control, similar to UDP. It allows
//! unlimited sending without backing off on packet loss. This is useful for:
//! - High-throughput applications where you control both endpoints
//! - Networks where you want maximum bandwidth regardless of loss
//! - Testing and benchmarking scenarios
//!
//! ⚠️ WARNING: This can be unfair to other network traffic and may cause
//! network congestion. Use only in controlled environments or where you
//! understand the implications.

use std::any::Any;
use std::sync::Arc;
use std::time::Instant;

use quinn::congestion::{Controller, ControllerFactory};
use quinn_proto::RttEstimator;

/// Configuration for the unlimited congestion controller.
#[derive(Debug, Clone)]
pub struct UnlimitedCongestionConfig {
    /// Initial congestion window size in bytes.
    /// This is the maximum amount of data that can be in flight.
    initial_window: u64,
}

impl Default for UnlimitedCongestionConfig {
    fn default() -> Self {
        Self {
            // Start with a very large window (1GB)
            initial_window: 1 << 30,
        }
    }
}

impl UnlimitedCongestionConfig {
    /// Create a new configuration with the default initial window.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the initial (and maximum) congestion window size in bytes.
    pub fn initial_window(mut self, window: u64) -> Self {
        self.initial_window = window;
        self
    }
}

impl ControllerFactory for UnlimitedCongestionConfig {
    fn build(self: Arc<Self>, _now: Instant, _current_mtu: u16) -> Box<dyn Controller> {
        Box::new(UnlimitedCongestionController { window: self.initial_window, ssthresh: u64::MAX })
    }
}

/// Unlimited congestion controller that never reduces its window.
///
/// This controller:
/// - Starts with a very large congestion window
/// - Never reduces the window on packet loss
/// - Ignores all congestion signals
/// - Behaves like UDP with no congestion control
#[derive(Debug, Clone)]
struct UnlimitedCongestionController {
    /// Current congestion window (always at maximum).
    window: u64,
    /// Slow start threshold (always at maximum).
    ssthresh: u64,
}

impl Controller for UnlimitedCongestionController {
    fn initial_window(&self) -> u64 {
        self.window
    }

    fn window(&self) -> u64 {
        self.window
    }

    fn clone_box(&self) -> Box<dyn Controller> {
        Box::new(self.clone())
    }

    fn on_ack(
        &mut self,
        _now: Instant,
        _sent: Instant,
        _bytes: u64,
        _app_limited: bool,
        _rtt: &RttEstimator,
    ) {
        // Do nothing on ACK - window stays at maximum
    }

    fn on_end_acks(
        &mut self,
        _now: Instant,
        _in_flight: u64,
        _app_limited: bool,
        _largest_packet_num_acked: Option<u64>,
    ) {
        // Do nothing at end of ACK processing
    }

    fn on_congestion_event(
        &mut self,
        _now: Instant,
        _sent: Instant,
        _is_persistent_congestion: bool,
        _lost_bytes: u64,
    ) {
        // Ignore congestion events - this is the key difference from other controllers
        // Window stays at maximum regardless of packet loss
    }

    fn on_mtu_update(&mut self, _new_mtu: u16) {
        // MTU changes don't affect our window
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

impl UnlimitedCongestionController {
    #[allow(dead_code)]
    fn ssthresh(&self) -> u64 {
        self.ssthresh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unlimited_never_reduces_window() {
        let config = Arc::new(UnlimitedCongestionConfig::default());
        let mut controller = config.build(Instant::now(), 1200);

        let initial_window = controller.window();
        assert_eq!(initial_window, 1 << 30);

        // Simulate congestion event
        controller.on_congestion_event(Instant::now(), Instant::now(), true, 10000);

        // Window should remain unchanged
        assert_eq!(controller.window(), initial_window);
    }

    #[test]
    fn test_unlimited_custom_window() {
        let config = Arc::new(UnlimitedCongestionConfig::new().initial_window(1 << 25));
        let controller = config.build(Instant::now(), 1200);

        assert_eq!(controller.window(), 1 << 25);
    }

    // Note: We can't test on_ack directly because RttEstimator::new is private.
    // The important behavior is tested through the congestion_event test below.

    #[test]
    fn test_unlimited_multiple_loss_events() {
        let config = Arc::new(UnlimitedCongestionConfig::default());
        let mut controller = config.build(Instant::now(), 1200);

        let initial_window = controller.window();

        // Simulate multiple severe loss events
        for _ in 0..10 {
            controller.on_congestion_event(Instant::now(), Instant::now(), true, 100000);
        }

        // Window should still be at maximum
        assert_eq!(controller.window(), initial_window);
    }
}
