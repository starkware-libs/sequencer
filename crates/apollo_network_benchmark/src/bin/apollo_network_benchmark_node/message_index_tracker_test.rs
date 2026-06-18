use super::MessageIndexTracker;

#[test]
fn empty_tracker_reports_zero_pending() {
    let tracker = MessageIndexTracker::default();
    assert_eq!(tracker.pending_messages_count(), 0);
}

#[test]
fn contiguous_deliveries_report_zero_pending() {
    let mut tracker = MessageIndexTracker::default();
    for index in 0..5 {
        tracker.seen_message(index);
    }
    assert_eq!(tracker.pending_messages_count(), 0);
}

#[test]
fn gap_in_deliveries_is_reported_as_pending() {
    let mut tracker = MessageIndexTracker::default();
    tracker.seen_message(0);
    tracker.seen_message(2);
    tracker.seen_message(4);
    // Range [0, 4] has 5 slots, we've seen 3, so 2 are pending.
    assert_eq!(tracker.pending_messages_count(), 2);
}

#[test]
fn out_of_order_deliveries_widen_the_range() {
    let mut tracker = MessageIndexTracker::default();
    tracker.seen_message(10);
    tracker.seen_message(5);
    tracker.seen_message(7);
    // Range [5, 10] has 6 slots, we've seen 3, so 3 are pending.
    assert_eq!(tracker.pending_messages_count(), 3);
}

#[test]
fn duplicates_saturate_to_zero_without_panicking() {
    let mut tracker = MessageIndexTracker::default();
    tracker.seen_message(7);
    tracker.seen_message(7);
    tracker.seen_message(7);
    // Range width is 1, but 3 receipts — saturate instead of underflowing.
    assert_eq!(tracker.pending_messages_count(), 0);
}
