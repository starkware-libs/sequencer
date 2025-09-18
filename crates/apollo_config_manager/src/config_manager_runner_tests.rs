use apollo_node_config::config_utils::DeploymentBaseAppConfig;
use apollo_node_config::definitions::ConfigPointersMap;
use apollo_node_config::node_config::{
    SequencerNodeConfig,
    CONFIG_NON_POINTERS_WHITELIST,
    CONFIG_POINTERS,
};
use tempfile::NamedTempFile;

/// Creates a temporary config file with specific test values and returns CLI args pointing to it.
fn create_temp_config_file_and_args() -> (NamedTempFile, Vec<String>) {
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

    // Create cli args pointing to the temp file
    let cli_args = vec![
        "test_node".to_string(),
        "--config_file".to_string(),
        temp_file.path().to_string_lossy().to_string(),
    ];

    (temp_file, cli_args)
}

#[tokio::test]
#[traced_test]
async fn test_update_config_with_changed_values() {
    let config_manager_client: SharedConfigManagerClient = Arc::new(MockConfigManagerClient::new());

    let initial_validator_id = "0x64";
    let (temp_file, cli_args) = create_temp_config_file_and_args(initial_validator_id);
    let config_manager_runner = ConfigManagerRunner::new(config_manager_client, cli_args);

    let result1 = config_manager_runner.update_config().await;
    assert!(result1.is_ok(), "First update_config should succeed");

    assert!(logs_contain("validator_id: ContractAddress(PatriciaKey(0x64))"));

    let new_validator_id = "0x128";

    let current_content =
        fs::read_to_string(temp_file.path()).expect("Failed to read temp config file");
    let updated_content = current_content.replace(initial_validator_id, new_validator_id);
    fs::write(temp_file.path(), updated_content)
        .expect("Failed to write updated config to temp file");

    let update_config_result_after_update = config_manager_runner.update_config().await;

    assert!(
        update_config_result_after_update.is_ok(),
        "Second update_config should succeed with changed values"
    );
    assert!(logs_contain("validator_id: ContractAddress(PatriciaKey(0x128))"));
}
