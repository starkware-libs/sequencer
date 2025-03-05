use starknet_infra_utils::dumping::serialize_to_file_test;

use crate::deployment_definitions::{MAIN_DEPLOYMENT, MAIN_DEPLOYMENT_PRESET_PATH};

/// Test that the deployment file is up to date. To update it run:
/// cargo run --bin deployment_generator -q
#[test]
fn default_dev_grafana_dashboard() {
    serialize_to_file_test(MAIN_DEPLOYMENT, MAIN_DEPLOYMENT_PRESET_PATH);
}
