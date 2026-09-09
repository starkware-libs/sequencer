use apollo_infra::metrics::HISTOGRAM_BUCKETS;

use super::MIN_LATENCY_BUCKET_SECONDS;

/// Verifies the bucket the alert selects is an existing histogram boundary.
#[test]
fn min_latency_bucket_is_a_real_histogram_boundary() {
    assert!(
        HISTOGRAM_BUCKETS.contains(&MIN_LATENCY_BUCKET_SECONDS),
        "{MIN_LATENCY_BUCKET_SECONDS} is not one of the configured buckets: {HISTOGRAM_BUCKETS:?}",
    );
}
