use std::env;
use std::fs::File;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::to_string_pretty;
use starknet_api::test_utils::json_utils::assert_json_eq;
use starknet_infra_utils::path::path_of_project_root;
use starknet_integration_tests::config_utils::SINGLE_NODE_CONFIG_PATH;
use tempfile::NamedTempFile;

#[test]
fn single_node_config_is_up_to_date() {
    let config_path: PathBuf = path_of_project_root().join(SINGLE_NODE_CONFIG_PATH);
    assert!(config_path.exists(), "Config file does not exist at expected path: {:?}", config_path);

    let from_config_file: serde_json::Value =
        serde_json::from_reader(File::open(&config_path).unwrap()).unwrap();

    // Use a named temporary file
    let tmp_file = NamedTempFile::new().expect("Failed to create temporary file");
    let tmp_file_path = tmp_file.path().to_path_buf();

    let binary_path = path_of_project_root().join("target/debug/dump_single_node_config");

    let mut child = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "system_test_dump_single_node_config",
            "--",
            "--config-output-path",
            tmp_file_path.to_str().unwrap(),
            "--db-dir",
            None,
        ])
        .stderr(Stdio::inherit())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Spawning system_test_dump_single_node_config should succeed.");

    let output = child.wait_with_output().expect("Failed to read output");
    assert!(output.status.success(), "Binary execution failed");

    assert!(tmp_file_path.exists(), "Temp config file was not created.");

    // Read and compare JSON
    let from_code: serde_json::Value =
        serde_json::from_reader(File::open(&tmp_file_path).unwrap()).unwrap();

    let from_config_file_str = to_string_pretty(&from_config_file).unwrap();
    let from_code_str = to_string_pretty(&from_code).unwrap();

    assert_eq!(from_config_file_str, from_code_str, "Single node config file is not up to date.");
}
