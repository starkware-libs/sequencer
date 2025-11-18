use apollo_metrics::{define_metrics, generate_permutation_labels};
use strum::{EnumVariantNames, VariantNames};
use strum_macros::IntoStaticStr;

define_metrics!(
    Blockifier => {
        LabeledMetricCounter { CLASS_CACHE_MISSES, "class_cache_misses", "Counter of global class cache misses", init=0, labels = BLOCKIFIER_CONTEXT_LABELS},
        LabeledMetricCounter { CLASS_CACHE_HITS, "class_cache_hits", "Counter of global class cache hits", init=0 , labels = BLOCKIFIER_CONTEXT_LABELS},
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

pub const LABEL_NAME_BLOCKIFIER_CONTEXT: &str = "blockifier_context";

#[derive(Clone, Copy, Debug, IntoStaticStr, EnumVariantNames)]
#[strum(serialize_all = "snake_case")]
pub enum BlockifierContext {
    Gateway,
    Batcher,
}

generate_permutation_labels! {
    BLOCKIFIER_CONTEXT_LABELS,
    (LABEL_NAME_BLOCKIFIER_CONTEXT, BlockifierContext),
}

pub(crate) fn record_class_cache_miss(reason: BlockifierContext) {
    CLASS_CACHE_MISSES.increment(1, &[(LABEL_NAME_BLOCKIFIER_CONTEXT, reason.into())]);
}

pub(crate) fn record_class_cache_hit(reason: BlockifierContext) {
    CLASS_CACHE_HITS.increment(1, &[(LABEL_NAME_BLOCKIFIER_CONTEXT, reason.into())]);
}
