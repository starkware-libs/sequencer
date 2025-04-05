use apollo_dashboard::alert_definitions::{DEV_ALERTS_JSON_PATH, SEQUENCER_ALERTS};
use apollo_dashboard::dashboard_definitions::{DEV_JSON_PATH, SEQUENCER_DASHBOARD};
use apollo_infra_utils::dumping::serialize_to_file;

/// Creates the dashboard json file.
fn main() {
    serialize_to_file(SEQUENCER_DASHBOARD, DEV_JSON_PATH);
    serialize_to_file(SEQUENCER_ALERTS, DEV_ALERTS_JSON_PATH);
}
