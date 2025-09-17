use std::fs;
use std::sync::Arc;

use apollo_config::CONFIG_FILE_ARG;
use apollo_config_manager_config::config::ConfigManagerConfig;
use apollo_config_manager_types::communication::{
    MockConfigManagerClient,
    SharedConfigManagerClient,
};
use apollo_consensus_config::config::ConsensusDynamicConfig;
use apollo_consensus_config::ValidatorId;
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

use crate::config_manager::ConfigManager;
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
async fn test_config_manager_runner_update_config_with_changed_values() {
    // Set a mock config manager client to expect the update dynamic config request.
    let mut mock_client = MockConfigManagerClient::new();
    mock_client.expect_update_dynamic_config().times(1..).return_const(Ok(()));
    let config_manager_client: SharedConfigManagerClient = Arc::new(mock_client);

    // Create a temporary config file and get the validator id value.
    let (temp_file, cli_args, validator_id_value) = create_temp_config_file_and_args();

    // Create a config manager runner and update the config.
    let config_manager_runner = ConfigManagerRunner::new(config_manager_client, cli_args);

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
        first_dynamic_config.consensus_dynamic_config.validator_id, expected_validator_id,
        "First update_config: Validator id mismatch: {} != {}",
        first_dynamic_config.consensus_dynamic_config.validator_id, expected_validator_id
    );

    // Edit the config file and then trigger a config update, expecting the new validator id.
    let new_validator_id = update_config_file(&temp_file);
    let expected_validator_id = ContractAddress::from(hex_to_u128(new_validator_id.as_str()));

    let second_update_config_result = config_manager_runner.update_config().await;
    let second_dynamic_config =
        second_update_config_result.expect("Second update_config should succeed");
    assert_eq!(
        second_dynamic_config.consensus_dynamic_config.validator_id, expected_validator_id,
        "Second update_config: Validator id mismatch: {} != {}",
        second_dynamic_config.consensus_dynamic_config.validator_id, expected_validator_id
    );
}

#[tokio::test]
async fn test_config_manager_update_config() {
    // Set a config manager.
    let config = ConfigManagerConfig::default();

    let consensus_dynamic_config = ConsensusDynamicConfig::default();
    let node_dynamic_config = NodeDynamicConfig { consensus_dynamic_config };
    let mut config_manager = ConfigManager::new(config, node_dynamic_config.clone());

    // Get the consensus dynamic config and assert it is the expected one.
    let consensus_dynamic_config = config_manager
        .get_consensus_dynamic_config()
        .expect("Failed to get consensus dynamic config");
    assert_eq!(
        consensus_dynamic_config, node_dynamic_config.consensus_dynamic_config,
        "Consensus dynamic config mismatch: {consensus_dynamic_config:#?} != {:#?}",
        node_dynamic_config.consensus_dynamic_config
    );

    // Set a new dynamic config by creating a new consensus dynamic config. For simplicity, we
    // create an arbitrary one and assert it's not the default one.
    let new_consensus_dynamic_config =
        ConsensusDynamicConfig { validator_id: ValidatorId::from(1_u8) };
    assert_ne!(
        consensus_dynamic_config, new_consensus_dynamic_config,
        "Consensus dynamic config should be different: {consensus_dynamic_config:#?} != {:#?}",
        new_consensus_dynamic_config
    );
    config_manager
        .set_node_dynamic_config(NodeDynamicConfig {
            consensus_dynamic_config: new_consensus_dynamic_config.clone(),
        })
        .expect("Failed to set node dynamic config");

    // Get the post-change consensus dynamic config and assert it is the expected one.
    let consensus_dynamic_config = config_manager
        .get_consensus_dynamic_config()
        .expect("Failed to get consensus dynamic config");
    assert_eq!(
        consensus_dynamic_config, new_consensus_dynamic_config,
        "Consensus dynamic config mismatch: {consensus_dynamic_config:#?} != {:#?}",
        new_consensus_dynamic_config
    );
}
