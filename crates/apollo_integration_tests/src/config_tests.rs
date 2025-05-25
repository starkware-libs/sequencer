use std::fs::File;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use apollo_infra_utils::path::resolve_project_relative_path;
use apollo_infra_utils::test_utils::assert_json_eq;
use serde_json::{from_reader, Value};
use tempfile::NamedTempFile;

const SINGLE_NODE_CONFIG_PATH: &str =
    "config/sequencer/presets/system_test_presets/single_node/node_0/executable_0/node_config.json";

/// Test that the single node preset is up to date. To update it, run:
/// cargo run --bin system_test_dump_single_node_config -q
#[test]
fn single_node_preset_is_up_to_date() {
    let config_path: PathBuf = resolve_project_relative_path(SINGLE_NODE_CONFIG_PATH).unwrap();
    assert!(config_path.exists(), "Config file does not exist at expected path: {:?}", config_path);

    // Current config path content.
    let from_config_file: Value = from_reader(File::open(&config_path).unwrap()).unwrap();

    // Use a named temporary file.
    let tmp_file = NamedTempFile::new().expect("temporary file should be created");
    let tmp_file_path = tmp_file.path().to_path_buf();
    let child = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "system_test_dump_single_node_config",
            "--",
            "--config-output-path",
            tmp_file_path.to_str().unwrap(),
        ])
        .stderr(Stdio::inherit())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Spawning system_test_dump_single_node_config should succeed.");

    let output = child.wait_with_output().expect("child process output should be available");
    assert!(output.status.success(), "Binary execution failed");

    assert!(tmp_file_path.exists(), "Temp config file was not created.");

    // Read and compare JSON
    let from_code: serde_json::Value =
        serde_json::from_reader(File::open(&tmp_file_path).unwrap()).unwrap();

    let error_message = "Single node config file is not up to date.";

    assert_json_eq(&from_config_file, &from_code, error_message.to_string());
}
