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
    monitor.mark_rejected();
    sleep(Duration::from_millis(30));
    // A second rejection extends the window instead of restarting it. Operators
    // care about how long the service has been rejecting, which is the time since
    // the first rejection.
    monitor.mark_rejected();
    assert!(monitor.saturated_for_at_least(Duration::from_millis(25)));
}

/// Backs both `mark_progress` call sites, worker-slot acquire and worker-slot release, which the
/// monitor cannot tell apart.
#[test]
fn mark_progress_clears_the_window() {
    let monitor = SaturationMonitor::default();
    monitor.mark_rejected();
    sleep(Duration::from_millis(10));
    monitor.mark_progress();
    assert!(!monitor.saturated_for_at_least(Duration::from_millis(0)));
}
