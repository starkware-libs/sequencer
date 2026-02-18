use apollo_metrics::define_metrics;

define_metrics!(
    Gateway => {
        MetricHistogram {
            PROOF_VERIFICATION_LATENCY,
            "proof_verification_latency",
            "Time taken to verify a proof in seconds"
        },
    },
    ConsensusOrchestrator => {
        MetricHistogram {
            CONSENSUS_PROOF_MANAGER_STORE_LATENCY,
            "consensus_proof_manager_store_latency",
            "Time taken to store a proof in the proof manager in seconds in the consensus orchestrator"
        },
    },
);
