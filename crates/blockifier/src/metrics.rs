use apollo_metrics::metrics::{MetricCounter, MetricDetails, MetricScope};
use apollo_metrics::{define_metrics, generate_permutation_labels};

use crate::bouncer::BouncerWeights;

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
        },
        LabeledMetricCounter {
            BLOCKS_FULL_BY_RESOURCE,
            "blockifier_blocks_full_by_resource",
            "Number of blocks closed on each bouncer resource",
            init = 0,
            labels = BLOCKS_FULL_BY_RESOURCE_LABELS
        }
    }
);

pub const LABEL_NAME_BLOCK_FULL_RESOURCE: &str = "resource";

generate_permutation_labels! {
    BLOCKS_FULL_BY_RESOURCE_LABELS,
    (LABEL_NAME_BLOCK_FULL_RESOURCE, BouncerWeights),
}

pub fn record_exceeded_bouncer_resources(exceeded_weights: &str) {
    for field in exceeded_weights.split(", ") {
        // Look up the static string from field_names() to satisfy the 'static lifetime requirement.
        let Some(static_field) =
            <BouncerWeights as strum::VariantNames>::VARIANTS.iter().find(|name| **name == field)
        else {
            continue;
        };
        BLOCKS_FULL_BY_RESOURCE.increment(1, &[(LABEL_NAME_BLOCK_FULL_RESOURCE, static_field)]);
    }
}

pub const BLOCKIFIER_METRIC_RATE_DURATION: &str = "5m";

pub struct CacheMetrics {
    misses: MetricCounter,
    hits: MetricCounter,
}

impl CacheMetrics {
    pub const fn new(misses: MetricCounter, hits: MetricCounter) -> Self {
        Self { misses, hits }
    }

    pub fn misses(&self) -> &MetricCounter {
        &self.misses
    }

    pub fn hits(&self) -> &MetricCounter {
        &self.hits
    }
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
