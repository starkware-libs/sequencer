/// Tracks how many messages a single sender has delivered and the range of indices seen.
///
/// Message indices are assumed to be assigned monotonically by the sender starting from 0.
/// `pending_messages_count()` reports the number of indices within the observed range that
/// have not yet been received, which serves as a proxy for in-flight messages. The tracker
/// intentionally does not dedupe — callers are expected to keep it cheap per message and
/// tolerate occasional noise from duplicate or reordered deliveries.
#[derive(Default, Clone, Copy)]
pub struct MessageIndexTracker {
    seen_messages_count: u64,
    max_message_index: Option<u64>,
    min_message_index: Option<u64>,
}

impl MessageIndexTracker {
    /// Records a single delivery of `message_index`. Duplicates are counted but do not widen
    /// the min/max range, which keeps `pending_messages_count` saturating to 0.
    pub fn seen_message(&mut self, message_index: u64) {
        self.seen_messages_count = self.seen_messages_count.saturating_add(1);
        if self.max_message_index.is_none_or(|max| max < message_index) {
            self.max_message_index = Some(message_index);
        }
        if self.min_message_index.is_none_or(|min| min > message_index) {
            self.min_message_index = Some(message_index);
        }
    }

    /// Returns the count of message indices within `[min, max]` that have not been observed.
    /// Returns 0 before any message is seen and when duplicates push the receipt count past
    /// the range width.
    pub fn pending_messages_count(&self) -> u64 {
        if self.seen_messages_count == 0 {
            return 0;
        }

        let min_message_index =
            self.min_message_index.expect("seen_messages_count > 0 implies min was set");
        let max_message_index =
            self.max_message_index.expect("seen_messages_count > 0 implies max was set");
        // `seen_messages_count` counts receipts, not distinct indices, so duplicates can
        // push it past the range width. Saturate to avoid underflow.
        (max_message_index - min_message_index + 1).saturating_sub(self.seen_messages_count)
    }
}

#[cfg(test)]
mod tests {
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
}
