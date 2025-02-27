use std::env;
use std::fs::File;

use colored::Colorize;
use starknet_api::test_utils::json_utils::assert_json_eq;
use starknet_infra_utils::path::resolve_project_relative_path;

use crate::dashboard_definitions::{DEV_JSON_PATH, SEQUENCER_DASHBOARD};

/// Test that the grafana dev dashboard file is up to date. To update the default config file, run:
/// cargo run --bin sequencer_dashboard_generator -q
#[test]
fn default_dev_grafana_dashboard() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");
    let loaded_dashboard: serde_json::Value =
        serde_json::from_reader(File::open(DEV_JSON_PATH).unwrap()).unwrap();

    let recreated_dashboard = serde_json::to_value(&SEQUENCER_DASHBOARD)
        .expect("Should have been able to serialize the dashboard to JSON");

    let error_message = format!(
        "{}\n{}",
        "Default config file doesn't match the default SequencerNodeConfig implementation. Please \
         update it using the sequencer_dump_config binary."
            .purple()
            .bold(),
        "Diffs shown below (loaded dashboard file <<>> dashboard serialization)."
    );
    assert_json_eq(&loaded_dashboard, &recreated_dashboard, error_message);
}
