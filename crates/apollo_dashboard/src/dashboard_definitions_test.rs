use apollo_infra_utils::dumping::serialize_to_file_test;

use crate::alert_definitions::{DEV_ALERTS_JSON_PATH, SEQUENCER_ALERTS};
use crate::dashboard_definitions::{DEV_JSON_PATH, SEQUENCER_DASHBOARD};

const FIX_BINARY_NAME: &str = "sequencer_dashboard_generator";

// Test that the grafana dev dashboard and alert files are up to date. To update the default config
// file, run: cargo run --bin sequencer_dashboard_generator -q
#[test]
fn default_dev_grafana_dashboard() {
    serialize_to_file_test(SEQUENCER_DASHBOARD, DEV_JSON_PATH, FIX_BINARY_NAME);
    serialize_to_file_test(SEQUENCER_ALERTS, DEV_ALERTS_JSON_PATH, FIX_BINARY_NAME);
}
