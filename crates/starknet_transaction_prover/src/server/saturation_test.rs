use std::thread::sleep;
use std::time::Duration;

use crate::server::saturation::SaturationMonitor;

#[test]
fn starts_healthy_before_any_traffic() {
    let monitor = SaturationMonitor::default();
    assert!(!monitor.saturated_for_at_least(Duration::from_millis(0)));
    assert!(!monitor.saturated_for_at_least(Duration::from_secs(10)));
}

#[test]
fn rejection_starts_window_and_threshold_eventually_passes() {
    let monitor = SaturationMonitor::default();
    // A proving run holds a worker slot while requests are rejected.
    monitor.mark_slot_acquired();
    monitor.mark_rejected();
    // The window has just opened, so the zero-elapsed comparison is still true. We are at
    // or past the 0ms threshold.
    assert!(monitor.saturated_for_at_least(Duration::from_millis(0)));
    // Not yet at the 50ms threshold, because the rejection happened just now.
    assert!(!monitor.saturated_for_at_least(Duration::from_millis(50)));
    sleep(Duration::from_millis(60));
    assert!(monitor.saturated_for_at_least(Duration::from_millis(50)));
}

#[test]
fn repeated_rejections_do_not_reset_the_window() {
    let monitor = SaturationMonitor::default();
    monitor.mark_slot_acquired();
    monitor.mark_rejected();
    sleep(Duration::from_millis(30));
    // A second rejection extends the window instead of restarting it. Operators
    // care about how long the service has been rejecting, which is the time since
    // the first rejection.
    monitor.mark_rejected();
    assert!(monitor.saturated_for_at_least(Duration::from_millis(25)));
}

/// A second run still holds a slot after the acquisition, so only the acquisition itself can
/// have closed the window.
#[test]
fn slot_acquisition_clears_the_window() {
    let monitor = SaturationMonitor::default();
    monitor.mark_slot_acquired();
    monitor.mark_rejected();
    monitor.mark_slot_acquired();
    assert!(!monitor.saturated_for_at_least(Duration::from_millis(0)));
}

/// One run finishes while another keeps its slot, so only the release itself can have closed the
/// window.
#[test]
fn slot_release_clears_the_window() {
    let monitor = SaturationMonitor::default();
    monitor.mark_slot_acquired();
    monitor.mark_slot_acquired();
    monitor.mark_rejected();
    monitor.mark_slot_released();
    assert!(!monitor.saturated_for_at_least(Duration::from_millis(0)));
}

/// A request that found the service full can record its reject after the last run released its
/// slot. Nothing is left to close the window it reopens, so an idle service must not report
/// saturation, or `/health` would stay at 503 until new traffic arrives.
#[test]
fn reject_after_last_slot_release_does_not_report_saturation() {
    let monitor = SaturationMonitor::default();
    monitor.mark_slot_acquired();
    monitor.mark_slot_released();
    monitor.mark_rejected();
    assert!(!monitor.saturated_for_at_least(Duration::from_millis(0)));
}
