    use std::fs;
    use std::sync::Arc;

    use apollo_config_manager_types::communication::{
        MockConfigManagerClient,
        SharedConfigManagerClient,
    };
    use apollo_node_config::config_utils::DeploymentBaseAppConfig;
    use apollo_node_config::definitions::ConfigPointersMap;
    use apollo_node_config::node_config::{SequencerNodeConfig, CONFIG_NON_POINTERS_WHITELIST, CONFIG_POINTERS};
    use serde_json::to_value;
    use starknet_api::core::ChainId;
    use starknet_api::contract_address;
    use tempfile::NamedTempFile;
    use tracing_test::traced_test;
    use url::Url;

    use crate::config_manager_runner::ConfigManagerRunner;

    /// Creates a temporary config file with specific test values and returns CLI args pointing to it.
    fn create_temp_config_file_and_args(validator_id: &str) -> (NamedTempFile, Vec<String>) {
        let config = SequencerNodeConfig::default();

        // Set up config pointers.
        let mut config_pointers_map = ConfigPointersMap::new(CONFIG_POINTERS.clone());

        // Set the required pointer targets
        config_pointers_map.change_target_value(
            "chain_id",
            to_value(ChainId::Other("SN_SEPOLIA".to_string())).expect("Failed to serialize ChainId"),
        );
        config_pointers_map.change_target_value(
            "validator_id",
            to_value(contract_address!(validator_id))
                .expect("Failed to serialize validator_id"),
        );
        config_pointers_map.change_target_value(
            "eth_fee_token_address",
            to_value(contract_address!("0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"))
                .expect("Failed to serialize eth_fee_token_address"),
        );
        config_pointers_map.change_target_value(
            "strk_fee_token_address",
            to_value(contract_address!("0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"))
                .expect("Failed to serialize strk_fee_token_address"),
        );
        config_pointers_map.change_target_value(
            "recorder_url",
            to_value(Url::parse("http://localhost:8080").expect("Invalid recorder_url"))
                .expect("Failed to serialize recorder_url"),
        );
        config_pointers_map.change_target_value(
            "starknet_url",
            to_value(Url::parse("http://localhost:8081").expect("Invalid starknet_url"))
                .expect("Failed to serialize starknet_url"),
        );

        let base_app_config = DeploymentBaseAppConfig::new(
            config,
            config_pointers_map,
            CONFIG_NON_POINTERS_WHITELIST.clone(),
        );

        // Create a temporary file
        let temp_file = NamedTempFile::new()
            .expect("Failed to create temporary config file");

        // Use the same method as integration tests to dump the config
        base_app_config.dump_config_file(temp_file.path());

        // Create CLI args pointing to the temp file
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

        // Test with initial validator_id = "0x64"
        let initial_validator_id = "0x64";
        let (temp_file, cli_args) = create_temp_config_file_and_args(initial_validator_id);
        let config_manager_runner = ConfigManagerRunner::new(config_manager_client, cli_args);

        let result1 = config_manager_runner.update_config().await;
        assert!(result1.is_ok(), "First update_config should succeed");

        assert!(logs_contain("validator_id: ContractAddress(PatriciaKey(0x64))"));

        let new_validator_id = "0x128";

        // Read the current config file content
        let current_content = fs::read_to_string(temp_file.path())
            .expect("Failed to read temp config file");

        // Replace the validator_id value in the file content
        let updated_content = current_content.replace(&initial_validator_id, new_validator_id);

        // Write the updated content back to the temp file
        fs::write(temp_file.path(), updated_content)
            .expect("Failed to write updated config to temp file");

        let result2 = config_manager_runner.update_config().await;

        assert!(result2.is_ok(), "Second update_config should succeed with changed values");
        assert!(logs_contain("validator_id: ContractAddress(PatriciaKey(0x128))"));

    }
