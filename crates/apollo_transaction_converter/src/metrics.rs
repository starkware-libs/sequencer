use apollo_metrics::{define_metrics, generate_permutation_labels};
use strum::{IntoStaticStr, VariantNames};

pub const LABEL_NAME_COMPONENT: &str = "component";

generate_permutation_labels! {
    PROOF_VERIFICATION_COMPONENT_LABELS,
    (LABEL_NAME_COMPONENT, ComponentLabelValue),
}

#[derive(Clone, Copy, Debug, IntoStaticStr, VariantNames)]
#[strum(serialize_all = "snake_case")]
pub enum ComponentLabelValue {
    Gateway,
    Consensus,
}

define_metrics!(
    Gateway => {
        MetricHistogram {
            PROOF_VERIFICATION_LATENCY,
            "proof_verification_latency",
            "Time taken to verify a proof in seconds"
        },
        LabeledMetricCounter {
            PROOF_VERIFICATION_COUNT,
            "proof_verification_count",
            "Number of proof verifications by component",
            init = 0,
            labels = PROOF_VERIFICATION_COMPONENT_LABELS
        },
    },
    ConsensusOrchestrator => {
        MetricHistogram {
            CONSENSUS_PROOF_MANAGER_STORE_LATENCY,
            "consensus_proof_manager_store_latency",
            "Time taken to store a proof in the proof manager during consensus orchestration, in seconds"
        },
    },
);
