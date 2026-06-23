use std::sync::Once;

use apollo_metrics::{define_metrics, generate_permutation_labels};
use strum::{EnumIter, IntoStaticStr, VariantNames};

pub const LABEL_NAME_SCRAPER: &str = "scraper";

/// Identifies which scraper component a metric pertains to.
#[derive(Clone, Copy, Debug, EnumIter, IntoStaticStr, VariantNames)]
#[strum(serialize_all = "snake_case")]
pub enum ScraperLabel {
    L1Events,
    L1GasPrice,
}

generate_permutation_labels! {
    SCRAPER_LABELS,
    (LABEL_NAME_SCRAPER, ScraperLabel),
}

define_metrics!(
    BaseLayer => {
        LabeledMetricGauge {
            L1_PRIMARY_ENDPOINT_DOWN_SINCE_TIMESTAMP_SECONDS,
            "l1_primary_endpoint_down_since_timestamp_seconds",
            "Unix timestamp (seconds) since which the primary L1 endpoint has been continuously \
             non-functional for the labeled scraper; 0 when the primary is healthy.",
            labels = SCRAPER_LABELS
        },
    },
);

pub fn register_metrics() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        L1_PRIMARY_ENDPOINT_DOWN_SINCE_TIMESTAMP_SECONDS.register();
    });
}
