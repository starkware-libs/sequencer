use std::fs;
use std::sync::Arc;

use apollo_config::CONFIG_FILE_ARG;
use apollo_config_manager_config::config::ConfigManagerConfig;
use apollo_config_manager_types::communication::{
    MockConfigManagerClient,
    SharedConfigManagerClient,
};
use apollo_consensus_config::config::ConsensusDynamicConfig;
use apollo_node_config::config_utils::DeploymentBaseAppConfig;
use apollo_node_config::definitions::ConfigPointersMap;
use apollo_node_config::node_config::{
    NodeDynamicConfig,
    SequencerNodeConfig,
    CONFIG_NON_POINTERS_WHITELIST,
    CONFIG_POINTERS,
};
use serde_json::Value;
use starknet_api::core::ContractAddress;
use tempfile::NamedTempFile;
use tracing_test::traced_test;

use crate::config_manager_runner::ConfigManagerRunner;

// An arbitrary hex-str config entry to be replaced.
const VALIDATOR_ID_CONFIG_ENTRY: &str = "validator_id";

/// Creates a temporary config file with specific test values and returns CLI args pointing to it.
fn create_temp_config_file_and_args() -> (NamedTempFile, Vec<String>, String) {
    let config = SequencerNodeConfig::default();
    let config_pointers_map = ConfigPointersMap::create_for_testing(CONFIG_POINTERS.clone());

    let base_app_config = DeploymentBaseAppConfig::new(
        config,
        config_pointers_map,
        CONFIG_NON_POINTERS_WHITELIST.clone(),
    );

    // Create a temporary file
    let temp_file = NamedTempFile::new().expect("Failed to create temporary config file");

    base_app_config.dump_config_file(temp_file.path());

    let current_validator_id = base_app_config
        .as_value()
        .get(VALIDATOR_ID_CONFIG_ENTRY)
        .and_then(|v| v.as_str())
        .expect("Missing or non-string hex value at VALIDATOR_ID_CONFIG_ENTRY")
        .to_string();

    // Create cli args pointing to the temp file
    let cli_args = vec![
        "test_node".to_string(),
        CONFIG_FILE_ARG.to_string(),
        temp_file.path().to_string_lossy().to_string(),
    ];

    (temp_file, cli_args, current_validator_id)
}

fn update_config_file(temp_file: &NamedTempFile) -> String {
    let current_content =
        fs::read_to_string(temp_file.path()).expect("Failed to read temp config file");

    // Parse JSON (expects a top-level object/map)
    let mut root: Value = serde_json::from_str(&current_content).expect("Config is not valid JSON");
    let obj = root.as_object_mut().expect("Config root must be a JSON object");

    // Get the hex string at the key VALIDATOR_ID_CONFIG_ENTRY (e.g., "validator_id": "0x00ff")
    let current_validator_id = obj
        .get(VALIDATOR_ID_CONFIG_ENTRY)
        .and_then(|v| v.as_str())
        .expect("Missing or non-string hex value at VALIDATOR_ID_CONFIG_ENTRY");
    assert!(current_validator_id.starts_with("0x"), "Expected a 0x-prefixed hex string");

    // Bump by 1 and preserve width
    let hex = &current_validator_id[2..]; // drop "0x"
    let n = u128::from_str_radix(hex, 16).unwrap() + 1;
    let new_validator_id = format!("0x{:0x}", n);

    // Update JSON and write back
    obj.insert(VALIDATOR_ID_CONFIG_ENTRY.to_string(), Value::String(new_validator_id.clone()));
    let updated_content = serde_json::to_string_pretty(&root).expect("Failed to serialize JSON");
    fs::write(temp_file.path(), updated_content)
        .expect("Failed to write updated config to temp file");

    new_validator_id
}

#[tokio::test]
async fn config_manager_runner_update_config_with_changed_values() {
    // Set a mock config manager client to expect the update dynamic config request.
    let mut mock_client = MockConfigManagerClient::new();
    mock_client.expect_set_node_dynamic_config().times(1..).return_const(Ok(()));
    let config_manager_client: SharedConfigManagerClient = Arc::new(mock_client);

    // Set a config manager config.
    let config_manager_config = ConfigManagerConfig::default();

    // Create a temporary config file and get the validator id value.
    let (temp_file, cli_args, validator_id_value) = create_temp_config_file_and_args();

    let node_dynamic_config = NodeDynamicConfig::default();

    // Create a config manager runner and update the config.
    let mut config_manager_runner = ConfigManagerRunner::new(
        config_manager_config,
        config_manager_client,
        node_dynamic_config,
        cli_args,
    );

    // Helper function to convert a hex string to a u128.
    fn hex_to_u128(s: &str) -> u128 {
        let hex = s.strip_prefix("0x").unwrap_or(s);
        u128::from_str_radix(hex, 16).unwrap()
    }

    // Trigger a config update, expecting the original validator id.
    let expected_validator_id = ContractAddress::from(hex_to_u128(validator_id_value.as_str()));

    let first_update_config_result = config_manager_runner.update_config().await;
    let first_dynamic_config =
        first_update_config_result.expect("First update_config should succeed");
    assert_eq!(
        first_dynamic_config.consensus_dynamic_config.as_ref().unwrap().validator_id,
        expected_validator_id,
        "First update_config: Validator id mismatch: {} != {}",
        first_dynamic_config.consensus_dynamic_config.as_ref().unwrap().validator_id,
        expected_validator_id
    );

    // Edit the config file and then trigger a config update, expecting the new validator id.
    let new_validator_id = update_config_file(&temp_file);
    let expected_validator_id = ContractAddress::from(hex_to_u128(new_validator_id.as_str()));

    let second_update_config_result = config_manager_runner.update_config().await;
    let second_dynamic_config =
        second_update_config_result.expect("Second update_config should succeed");
    assert_eq!(
        second_dynamic_config.consensus_dynamic_config.as_ref().unwrap().validator_id,
        expected_validator_id,
        "Second update_config: Validator id mismatch: {} != {}",
        second_dynamic_config.consensus_dynamic_config.as_ref().unwrap().validator_id,
        expected_validator_id
    );
}

#[traced_test]
#[test]
fn log_config_diff_changes() {
    let old_dynamic_config = NodeDynamicConfig {
        consensus_dynamic_config: Some(ConsensusDynamicConfig {
            validator_id: ContractAddress::from(1u128),
        }),
        ..Default::default()
    };

    let new_dynamic_config = NodeDynamicConfig {
        consensus_dynamic_config: Some(ConsensusDynamicConfig {
            validator_id: ContractAddress::from(2u128),
        }),
        ..Default::default()
    };

    let mock_client = MockConfigManagerClient::new();
    let runner = ConfigManagerRunner::new(
        ConfigManagerConfig::default(),
        Arc::new(mock_client),
        old_dynamic_config.clone(),
        Vec::<String>::new(),
    );

    runner.log_config_diff(&old_dynamic_config, &new_dynamic_config);

    assert!(logs_contain(
        r#"consensus_dynamic_config changed from {"validator_id":"0x1"} to {"validator_id":"0x2"}"#
    ));
}
