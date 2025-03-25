use apollo_infra_utils::dumping::serialize_to_file;
use starknet_sequencer_dashboard::alert_definitions::{DEV_ALERTS_JSON_PATH, SEQUENCER_ALERTS};
use starknet_sequencer_dashboard::dashboard_definitions::{DEV_JSON_PATH, SEQUENCER_DASHBOARD};

/// Creates the dashboard json file.
fn main() {
    serialize_to_file(SEQUENCER_DASHBOARD, DEV_JSON_PATH);
    serialize_to_file(SEQUENCER_ALERTS, DEV_ALERTS_JSON_PATH);
}
