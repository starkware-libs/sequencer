use apollo_metrics::{define_metrics, generate_permutation_labels};
use apollo_metrics::metrics::{MetricCounter, MetricDetails, MetricScope};
use strum::{EnumVariantNames, IntoStaticStr};

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

#[derive(Clone, Copy, Debug, IntoStaticStr, EnumVariantNames)]
#[strum(serialize_all = "snake_case")]
pub enum BlockFullResource {
    L1Gas,
    MessageSegmentLength,
    NEvents,
    StateDiffSize,
    SierraGas,
    NTxs,
    ProvingGas,
}

generate_permutation_labels! {
    BLOCKS_FULL_BY_RESOURCE_LABELS,
    (LABEL_NAME_BLOCK_FULL_RESOURCE, BlockFullResource),
}

fn record_block_full_by_resource(resource: BlockFullResource) {
    BLOCKS_FULL_BY_RESOURCE.increment(1, &[(LABEL_NAME_BLOCK_FULL_RESOURCE, resource.into())]);
}

pub fn record_exceeded_bouncer_resources(exceeded_weights: &str) {
    for field in exceeded_weights.split(", ") {
        let resource = match field {
            "l1_gas" => BlockFullResource::L1Gas,
            "message_segment_length" => BlockFullResource::MessageSegmentLength,
            "n_events" => BlockFullResource::NEvents,
            "state_diff_size" => BlockFullResource::StateDiffSize,
            "sierra_gas" => BlockFullResource::SierraGas,
            "n_txs" => BlockFullResource::NTxs,
            "proving_gas" => BlockFullResource::ProvingGas,
            _ => continue,
        };
        record_block_full_by_resource(resource);
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
