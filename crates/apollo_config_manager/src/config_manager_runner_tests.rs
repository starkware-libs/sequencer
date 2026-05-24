use std::fs;
use std::sync::Arc;
use std::time::Duration;

use apollo_config::CONFIG_FILE_ARG;
use apollo_config_manager_config::config::ConfigManagerConfig;
use apollo_config_manager_types::communication::{
    MockConfigManagerClient,
    SharedConfigManagerClient,
};
use apollo_consensus_config::config::ConsensusDynamicConfig;
use apollo_node_config::config_utils::DeploymentBaseAppConfig;
use apollo_node_config::node_config::{NodeDynamicConfig, SequencerNodeConfig};
use serde_json::Value;
use starknet_api::core::ContractAddress;
use tempfile::NamedTempFile;
use tokio::sync::mpsc::channel;
use tokio::task::yield_now;
use tokio::time::{interval, timeout};
use tracing_test::traced_test;

use crate::config_manager_runner::ConfigManagerRunner;

// Nested path to validator_id in the sequencer node config.
const VALIDATOR_ID_PATH: &[&str] =
    &["consensus_manager_config", "consensus_manager_config", "dynamic_config", "validator_id"];
const TEST_TIMEOUT_SECS: u64 = 1;

/// Reads a string value at the given dotted path in a nested JSON object.
fn get_nested_str<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for key in path {
        current = current.get(key)?;
    }
    current.as_str()
}

/// Sets a string value at the given dotted path in a nested JSON object, creating intermediate
/// objects if needed.
fn set_nested_str(value: &mut Value, path: &[&str], new_value: String) {
    let (last, parents) = path.split_last().expect("path must be non-empty");
    let mut current = value;
    for key in parents {
        current = current
            .as_object_mut()
            .expect("expected object")
            .entry(*key)
            .or_insert(Value::Object(Default::default()));
    }
    current.as_object_mut().expect("expected object").insert((*last).to_string(), new_value.into());
}

/// Creates a temporary config file with specific test values and returns CLI args pointing to it.
fn create_temp_config_file_and_args() -> (NamedTempFile, Vec<String>, String) {
    let config = SequencerNodeConfig::default();
    let base_app_config = DeploymentBaseAppConfig::new(config);

    let temp_file = NamedTempFile::new().expect("Failed to create temporary config file");
    base_app_config.dump_config_file(temp_file.path());

    let current_validator_id = get_nested_str(&base_app_config.as_value(), VALIDATOR_ID_PATH)
        .expect("Missing or non-string hex value at VALIDATOR_ID_PATH")
        .to_string();

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

    let mut root: Value = serde_json::from_str(&current_content).expect("Config is not valid JSON");

    let current_validator_id = get_nested_str(&root, VALIDATOR_ID_PATH)
        .expect("Missing or non-string hex value at VALIDATOR_ID_PATH");
    assert!(current_validator_id.starts_with("0x"), "Expected a 0x-prefixed hex string");

    let hex = &current_validator_id[2..]; // drop "0x"
    let n = u128::from_str_radix(hex, 16).unwrap() + 1;
    let new_validator_id = format!("0x{n:0x}");

    set_nested_str(&mut root, VALIDATOR_ID_PATH, new_validator_id.clone());
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

#[tokio::test]
async fn watcher_triggers_update_on_file_change() {
    // Prepare temp config file and CLI args.
    let (temp_file, cli_args, _) = create_temp_config_file_and_args();

    // Channel to observe that update_config was called.
    let (tx, mut rx) = channel(1);

    let mut mock_client = MockConfigManagerClient::new();
    mock_client.expect_set_node_dynamic_config().times(1).returning(move |_| {
        let _ = tx.blocking_send(());
        Ok(())
    });

    let client: SharedConfigManagerClient = Arc::new(mock_client);

    let mut runner = ConfigManagerRunner::new(
        ConfigManagerConfig::default(),
        client,
        NodeDynamicConfig::default(),
        cli_args,
    );

    // Spawn watcher loop in background task.
    tokio::spawn(async move {
        let _ = runner.run_watcher_loop(interval(Duration::MAX)).await;
    });

    yield_now().await;

    // Modify the config file to trigger an event.
    let _ = update_config_file(&temp_file);

    // Wait until the update call is observed or timeout.
    timeout(Duration::from_secs(TEST_TIMEOUT_SECS), rx.recv())
        .await
        .expect("update_config was not called within timeout");
}

#[traced_test]
#[test]
fn log_config_diff_changes() {
    let old_dynamic_config = NodeDynamicConfig {
        consensus_dynamic_config: Some(ConsensusDynamicConfig {
            validator_id: ContractAddress::from(1u128),
            ..Default::default()
        }),
        ..Default::default()
    };

    let new_dynamic_config = NodeDynamicConfig {
        consensus_dynamic_config: Some(ConsensusDynamicConfig {
            validator_id: ContractAddress::from(2u128),
            ..Default::default()
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

    assert!(logs_contain("consensus_dynamic_config changed from"));
    assert!(logs_contain(r#""validator_id":"0x1""#));
    assert!(logs_contain(r#""validator_id":"0x2""#));
}
