use crate::metrics::MetricCounter;

/// Holds a `MetricCounter`, and when dropped, increments the counter by 1.
/// To prevent the increment, call `defuse`.
/// This is useful for scenarios where you want to ensure a metric is incremented
/// only if certain conditions are met, such as when a task completes successfully.
/// Example usage:
/// ```rust
/// let mut bomb = MetricCounterBomb::new(MY_METRIC_COUNTER);
/// // ... some complicated logic that may return an error or panic...
/// bomb.defuse(); // Prevent the metric from being incremented
/// retrun Ok(());
/// ```
pub struct MetricCounterBomb {
    pub metric: MetricCounter,
    pub primed: bool,
}

impl MetricCounterBomb {
    pub fn new(metric: MetricCounter) -> Self {
        Self { metric, primed: true }
    }

    pub fn defuse(&mut self) {
        self.primed = false;
    }
}

impl Drop for MetricCounterBomb {
    fn drop(&mut self) {
        if self.primed {
            self.metric.increment(1);
        }
    }
}
