use apollo_metrics::define_metrics;
use apollo_metrics::metrics::MetricCounter;

define_metrics!(
    ApolloStateReader => {
        MetricCounter { CLASS_CACHE_MISSES, "class_cache_misses", "Counter of global class cache misses", init=0 },
        MetricCounter { CLASS_CACHE_HITS, "class_cache_hits", "Counter of global class cache hits", init=0 },
        MetricCounter {
            NATIVE_CLASS_RETURNED,
            "native_class_returned",
            "Counter of the number of times that the state reader returned Native class",
            init=0}
    }
);

pub const STATE_READER_METRIC_RATE_DURATION: &str = "5m";
