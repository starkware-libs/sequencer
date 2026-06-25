use std::sync::Arc;
use std::time::Duration;

use apollo_config::CONFIG_FILE_ARG;
use apollo_config_manager_config::config::ConfigManagerConfig;
use apollo_config_manager_types::communication::{
    MockConfigManagerClient,
    SharedConfigManagerClient,
};
use apollo_consensus_config::config::ConsensusDynamicConfig;
use apollo_node_config::config_utils::normalize_pointer_groups;
use apollo_node_config::node_config::{NodeDynamicConfig, SequencerNodeConfig};
use serde_json::Value;
use starknet_api::core::ContractAddress;
use tempfile::TempDir;
use tokio::sync::mpsc::channel;
use tokio::task::yield_now;
use tokio::time::{interval, timeout};
use tracing_test::traced_test;

use crate::config_manager_runner::ConfigManagerRunner;

// The nested native path of the validator id within the `SequencerNodeConfig` field hierarchy.
const VALIDATOR_ID_CONFIG_PATH: &[&str] =
    &["consensus_manager_config", "consensus_manager_config", "dynamic_config", "validator_id"];
const TEST_TIMEOUT_SECS: u64 = 1;
const BASE_CONFIG_FILE_NAME: &str = "base_config.json";
const SECRETS_CONFIG_FILE_NAME: &str = "secrets_config.json";

// `Sensitive<T>` fields whose derived `Serialize` emits an asymmetric wire shape (a JSON array or
// `null`) that their string-reading `deserialize_with` cannot consume, and whose deserializer maps
// the empty string back to `None`. A whole-config `to_value`/`from_value` round-trip fails on them
// unless they are rewritten to that empty string (see
// `apollo_node_config::config_serde_symmetry_test` and `apollo_config::converters`). The native
// node loader avoids this in production by overlaying real values from the secrets file; this test
// carries no secrets, so it neutralizes them instead.
const NONE_ABLE_SENSITIVE_KEYS: &[&str] = &["secret_key", "url_header_list"];

// `EthereumBaseLayerConfig::ordered_l1_endpoint_urls: Vec<Sensitive<Url>>` shares the same
// serialize-array/deserialize-string asymmetry, but its deserializer reads a (space-separated) URL
// list and `validate_node_config` rejects an empty list, so it has no `None`/empty form to
// substitute. Rewrite it to a single valid URL string so the config both round-trips and validates.
const ENDPOINT_URLS_KEY: &str = "ordered_l1_endpoint_urls";
const PLACEHOLDER_ENDPOINT_URL: &str = "https://localhost:8545";

/// Recursively rewrites the asymmetric `Sensitive` fields in a nested native config map to the
/// string wire shape their deserializers read, so the config round-trips through serde and passes
/// validation.
fn neutralize_sensitive_fields(config: &mut Value) {
    match config {
        Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if NONE_ABLE_SENSITIVE_KEYS.contains(&key.as_str()) {
                    *value = Value::String(String::new());
                } else if key == ENDPOINT_URLS_KEY {
                    *value = Value::String(PLACEHOLDER_ENDPOINT_URL.to_string());
                } else {
                    neutralize_sensitive_fields(value);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(neutralize_sensitive_fields),
        _ => {}
    }
}

/// Returns a mutable reference to the leaf `validator_id` value within a nested native config map.
fn validator_id_entry(config: &mut Value) -> &mut Value {
    let mut entry = config;
    for segment in VALIDATOR_ID_CONFIG_PATH {
        entry = entry
            .as_object_mut()
            .expect("Native config node must be a JSON object")
            .get_mut(*segment)
            .unwrap_or_else(|| panic!("Missing native config segment {segment:?}"));
    }
    entry
}

/// Creates a temporary directory holding a native base config file and an (empty) secrets file,
/// and returns CLI args pointing the native loader at both. The directory handle is returned so the
/// files outlive the test. Also returns the validator id read from the base config.
fn create_temp_config_files_and_args() -> (TempDir, Vec<String>, String) {
    let mut config = SequencerNodeConfig::default();
    // `SequencerNodeConfig::default()` does not have internally consistent pointer-group values
    // (chain_id, fee tokens, etc.); reconcile them so the loaded config passes the cross-component
    // equality validation, mirroring what pointer resolution did at load time.
    normalize_pointer_groups(&mut config);
    let mut base_config =
        serde_json::to_value(&config).expect("Should be able to serialize config to value");
    neutralize_sensitive_fields(&mut base_config);

    let validator_id = validator_id_entry(&mut base_config)
        .as_str()
        .expect("validator_id must be a string")
        .to_string();

    let temp_dir = TempDir::new().expect("Failed to create temporary config dir");
    let base_config_path = temp_dir.path().join(BASE_CONFIG_FILE_NAME);
    let secrets_config_path = temp_dir.path().join(SECRETS_CONFIG_FILE_NAME);

    let base_content =
        serde_json::to_string_pretty(&base_config).expect("Failed to serialize base config");
    std::fs::write(&base_config_path, base_content).expect("Failed to write base config file");
    // The native loader requires a secrets file overlaid onto the base; the test carries none.
    std::fs::write(&secrets_config_path, "{}").expect("Failed to write secrets config file");

    // The native loader expects two files: the nested base config and the flat secrets config.
    let cli_args = vec![
        "test_node".to_string(),
        CONFIG_FILE_ARG.to_string(),
        base_config_path.to_string_lossy().to_string(),
        CONFIG_FILE_ARG.to_string(),
        secrets_config_path.to_string_lossy().to_string(),
    ];

    (temp_dir, cli_args, validator_id)
}

/// Bumps the validator id in the native base config file by one and returns the new value.
fn update_config_file(temp_dir: &TempDir) -> String {
    let base_config_path = temp_dir.path().join(BASE_CONFIG_FILE_NAME);
    let current_content =
        std::fs::read_to_string(&base_config_path).expect("Failed to read base config file");

    let mut base_config: Value =
        serde_json::from_str(&current_content).expect("Config is not valid JSON");

    let validator_id_value = validator_id_entry(&mut base_config);
    let current_validator_id = validator_id_value.as_str().expect("validator_id must be a string");
    assert!(current_validator_id.starts_with("0x"), "Expected a 0x-prefixed hex string");

    // Bump by 1 and preserve width.
    let hex = &current_validator_id[2..]; // drop "0x"
    let n = u128::from_str_radix(hex, 16).unwrap() + 1;
    let new_validator_id = format!("0x{:0x}", n);

    *validator_id_value = Value::String(new_validator_id.clone());
    let updated_content =
        serde_json::to_string_pretty(&base_config).expect("Failed to serialize JSON");
    std::fs::write(&base_config_path, updated_content)
        .expect("Failed to write updated config to base config file");

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

    // Create temporary config files and get the validator id value.
    let (temp_dir, cli_args, validator_id_value) = create_temp_config_files_and_args();

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
    let new_validator_id = update_config_file(&temp_dir);
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
    // Prepare temp config files and CLI args.
    let (temp_dir, cli_args, _) = create_temp_config_files_and_args();

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
    let _ = update_config_file(&temp_dir);

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
