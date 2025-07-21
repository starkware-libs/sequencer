use apollo_infra_utils::dumping::serialize_to_file_test;
use strum::IntoEnumIterator;

use crate::alert_definitions::{get_apollo_alerts, get_dev_alerts_json_path};
use crate::alerts::AlertEnvFiltering;
use crate::dashboard_definitions::{get_apollo_dashboard, DEV_JSON_PATH};

const FIX_BINARY_NAME: &str = "sequencer_dashboard_generator";

// Test that the grafana dev dashboard and alert files are up to date. To update the default config
// file, run: cargo run --bin sequencer_dashboard_generator -q
#[test]
fn default_dev_grafana_dashboard() {
    serialize_to_file_test(get_apollo_dashboard(), DEV_JSON_PATH, FIX_BINARY_NAME);
    for alert_env_filtering in AlertEnvFiltering::iter() {
        if alert_env_filtering == AlertEnvFiltering::All {
            continue; // Skip the 'All' variant, as it used to cover all other options.
        }
        serialize_to_file_test(
            get_apollo_alerts(alert_env_filtering),
            &get_dev_alerts_json_path(alert_env_filtering),
            FIX_BINARY_NAME,
        );
    }
}
