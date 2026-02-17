use apollo_metrics::define_metrics;

define_metrics!(
    Gateway => {
        MetricHistogram {
            PROOF_VERIFICATION_LATENCY,
            "proof_verification_latency",
            "Time taken to verify a proof in seconds"
        },
    },
);
