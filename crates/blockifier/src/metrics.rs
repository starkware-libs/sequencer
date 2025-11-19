use apollo_metrics::define_metrics;
use apollo_metrics::metrics::{MetricCounter, MetricDetails, MetricScope};

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

pub struct CacheMetrics {
    pub misses: MetricCounter,
    pub hits: MetricCounter,
}

impl CacheMetrics {
    pub fn register(&self) {
        self.misses.register();
        self.hits.register();
    }

    pub fn increment_miss(&self) {
        self.misses.increment(1);
    }

    pub fn increment_hit(&self) {
        self.hits.increment(1);
    }

    pub fn get_scope(&self) -> MetricScope {
        assert_eq!(
            self.misses.get_scope(),
            self.hits.get_scope(),
            "Scope of misses and hits must be the same"
        );
        self.misses.get_scope()
    }
}
