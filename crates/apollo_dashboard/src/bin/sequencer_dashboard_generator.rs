use apollo_dashboard::alert_definitions::{get_apollo_alerts, DEV_ALERTS_JSON_PATH};
use apollo_dashboard::dashboard_definitions::{get_apollo_dashboard, DEV_JSON_PATH};
use apollo_infra_utils::dumping::serialize_to_file;

/// Creates the dashboard json file.
fn main() {
    serialize_to_file(get_apollo_dashboard(), DEV_JSON_PATH);
    serialize_to_file(get_apollo_alerts(), DEV_ALERTS_JSON_PATH);
}
