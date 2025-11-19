use apollo_metrics::define_metrics;
use apollo_metrics::metrics::MetricCounter;
#[cfg(any(test, feature = "testing"))]
use mockall::automock;

define_metrics!(
    Blockifier => {
        MetricCounter {
            NATIVE_CLASS_RETURNED,
            "native_class_returned",
            "Counter of the number of times that the state reader returned Native class",
            init=0},
        MetricCounter { NATIVE_COMPILATION_ERROR,
            "native_compilation_error",
            "Counter of Native compilation failures in the blockifier",
            init=0 },
        MetricCounter {
            CALLS_RUNNING_NATIVE,
            "calls_running_native",
            "Counter of the number of calls running native",
            init=0
        },
        MetricCounter {
            TOTAL_CALLS,
            "number_of_total_calls",
            "Counter of the total number of calls",
            init=0
        }
    }
);

pub const BLOCKIFIER_METRIC_RATE_DURATION: &str = "5m";

#[cfg_attr(any(test, feature = "testing"), automock)]
pub trait ClassCacheMetricsTrait: std::marker::Sized {
    fn register(&self);
    fn increment_miss(&self);
    fn increment_hit(&self);
}

pub struct ClassCacheMetrics {
    pub misses: MetricCounter,
    pub hits: MetricCounter,
}

impl ClassCacheMetricsTrait for ClassCacheMetrics {
    fn register(&self) {
        self.misses.register();
        self.hits.register();
    }

    fn increment_miss(&self) {
        self.misses.increment(1);
    }

    fn increment_hit(&self) {
        self.hits.increment(1);
    }
}

#[cfg(any(test, feature = "testing"))]
pub fn mock_class_cache_metrics() -> MockClassCacheMetricsTrait {
    let mut class_cache_metrics = MockClassCacheMetricsTrait::new();
    class_cache_metrics.expect_increment_miss().times(..).return_const(());
    class_cache_metrics.expect_increment_hit().times(..).return_const(());
    class_cache_metrics
}
