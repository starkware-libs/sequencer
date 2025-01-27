use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use starknet_infra_utils::path::resolve_project_relative_path;
use starknet_sequencer_metrics::dashboard_definitions::DASHBOARD_EXAMPLE;

const DASHBOARD_FILE_PATH: &str = "Monitoring/sequencer/grafana_data.json";

// TODO(Tsabary): create a test that ensures the dashboard file is updated.

/// Creates the dashboard json file.
fn main() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");

    // Serialize the dashboard to a JSON string.
    let json_string = serde_json::to_string_pretty(&DASHBOARD_EXAMPLE)
        .expect("Failed to serialize struct to JSON");

    // Write the JSON string to a file.
    let mut file = File::create(DASHBOARD_FILE_PATH).expect("Failed to create file");
    file.write_all(json_string.as_bytes()).unwrap();

    // Add an extra newline after the JSON content.
    file.write_all(b"\n").unwrap();

    assert!(
        PathBuf::from(DASHBOARD_FILE_PATH).exists(),
        "Failed creating grafana data file: {:?}",
        DASHBOARD_FILE_PATH
    );
}
