use std::{env, fs};

use serde_json::{from_str, Value};
use starknet_infra_utils::dumping::serialize_to_file_test;
use starknet_infra_utils::path::resolve_project_relative_path;

use crate::deployment::DeploymentAndPreset;
use crate::deployment_definitions::{DEPLOYMENTS, SINGLE_NODE_CONFIG_PATH};

/// Test that the deployment file is up to date. To update it run:
/// cargo run --bin deployment_generator -q
#[test]
fn deployment_files_are_up_to_date() {
    for deployment_fn in DEPLOYMENTS {
        let DeploymentAndPreset { deployment, dump_file_path } = deployment_fn();
        serialize_to_file_test(deployment, dump_file_path);
    }
}

#[test]
fn application_config_files_exist() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");
    for deployment_fn in DEPLOYMENTS {
        let DeploymentAndPreset { deployment, dump_file_path: _ } = deployment_fn();
        deployment.assert_application_configs_exist();
    }
}

// This is a temporary test that ensures the configs used in the system and integration tests are
// identical. This is a temporary workaround until the adequate dump functions will be set.
// If this test fails:
// 1. make sure the "deployment_files_are_up_to_date" test above passes
// 2. copy "file1_path" into "file2_path"
// TODO(Tsabary/Nadin): delete this test once app config is generated in the appropriate location.
#[test]
fn copied_consolidated_node_application_config_is_up_to_date() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");
    // Define the paths to the two JSON files
    let file1_path = SINGLE_NODE_CONFIG_PATH;
    let file2_path = "config/sequencer/presets/consolidated_node/application_configs/node.json";

    // Check if files exist
    assert!(fs::metadata(file1_path).is_ok(), "File1 does not exist");
    assert!(fs::metadata(file2_path).is_ok(), "File2 does not exist");

    // Read file contents
    let file1_content = fs::read_to_string(file1_path).expect("Failed to read file1");
    let file2_content = fs::read_to_string(file2_path).expect("Failed to read file2");

    // Parse JSON into serde_json::Value
    let json1: Value = from_str(&file1_content).expect("Failed to parse JSON from file1");
    let json2: Value = from_str(&file2_content).expect("Failed to parse JSON from file2");

    // Compare the JSON structures
    assert_eq!(json1, json2, "JSON files are not identical");
}
