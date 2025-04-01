use apollo_sequencer_metrics::define_metrics;
use apollo_sequencer_metrics::metrics::MetricCounter;

define_metrics!(
    ApolloStateReader => {
        MetricCounter { CLASS_CACHE_MISSES, "class_cache_misses", "Counter of global class cache misses", init=0 },
        MetricCounter { CLASS_CACHE_HITS, "class_cache_hits", "Counter of global class cache hits", init=0 }
    }
);
