use apollo_metrics::define_metrics;

define_metrics!(
    Blockifier => {
        MetricCounter { CLASS_CACHE_MISSES, "class_cache_misses", "Counter of global class cache misses", init=0 },
        MetricCounter { CLASS_CACHE_HITS, "class_cache_hits", "Counter of global class cache hits", init=0 },
        MetricCounter {
            NATIVE_CLASS_RETURNED,
            "native_class_returned",
            "Counter of the number of times that the state reader returned Native class",
            init=0},
        MetricCounter { NATIVE_COMPILATION_ERROR,
            "native_compilation_error",
            "Counter of Native compilation failures in the blockifier",
            init=0 },
        MetricGauge {
            CALLS_RUNNING_NATIVE_RATE,
            "calls_running_native_rate",
            "Gauge of the rate of calls running native"
        },
    }
);

pub const STATE_READER_METRIC_RATE_DURATION: &str = "5m";
