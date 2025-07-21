use apollo_dashboard::alert_definitions::{get_apollo_alerts, get_dev_alerts_json_path};
use apollo_dashboard::alerts::AlertEnvFiltering;
use apollo_dashboard::dashboard_definitions::{get_apollo_dashboard, DEV_JSON_PATH};
use apollo_infra_utils::dumping::serialize_to_file;
use strum::IntoEnumIterator;

/// Creates the dashboard and alerts json files.
fn main() {
    serialize_to_file(get_apollo_dashboard(), DEV_JSON_PATH);
    for alert_env_filtering in AlertEnvFiltering::iter() {
        if alert_env_filtering == AlertEnvFiltering::All {
            continue; // Skip the 'All' variant, as it used to cover all other options.
        }
        serialize_to_file(
            get_apollo_alerts(alert_env_filtering),
            &get_dev_alerts_json_path(alert_env_filtering),
        );
    }
}
