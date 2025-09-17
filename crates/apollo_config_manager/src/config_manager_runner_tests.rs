use std::fs;
use std::sync::Arc;

use apollo_config_manager_types::communication::{ConfigManagerClient, MockConfigManagerClient};
use apollo_infra_utils::path::resolve_project_relative_path;
use serde_json::json;
use tempfile::NamedTempFile;
use tracing_test::traced_test;

use crate::config_manager_runner::ConfigManagerRunner;

// TODO(Nadin): Import this constant from apollo_deployments once circular dependency is resolved
const SECRETS_FOR_TESTING_ENV_PATH: &str =
    "crates/apollo_deployments/resources/testing_secrets.json";

fn create_cli_args_from_deployment_system() -> Vec<String> {
    let workspace_root = resolve_project_relative_path(".").expect("Failed to get project root");
    let mut cli_args = vec!["test_program".to_string()];

    // Read the deployment config to get the exact same config paths used by the deployment
    // system
    // TODO(Nadin): Use Environment::LocalK8s.env_dir_path() from apollo_deployments once circular
    // dependency is resolved
    let deployment_config_path = workspace_root
        .join("crates/apollo_deployments/resources/deployments/testing")
        .join("deployment_config_consolidated.json");

    let deployment_config_content =
        fs::read_to_string(&deployment_config_path).expect("Failed to read deployment config");

    let deployment_config: serde_json::Value = serde_json::from_str(&deployment_config_content)
        .expect("Failed to parse deployment config");

    // Extract the application config subdir and config paths from the deployment system
    let app_config_subdir = deployment_config["application_config_subdir"]
        .as_str()
        .expect("Missing application_config_subdir");

    let config_paths = deployment_config["services"][0]["config_paths"]
        .as_array()
        .expect("Missing config_paths array");

    // Use the exact config paths defined by the deployment system
    for config_path in config_paths {
        let path_str = config_path.as_str().expect("Invalid config path");
        let full_path = workspace_root.join(app_config_subdir).join(path_str);

        cli_args.push("--config_file".to_string());
        cli_args.push(full_path.to_string_lossy().to_string());
    }

    // Add testing secrets (not in deployment config)
    cli_args.push("--config_file".to_string());
    cli_args.push(workspace_root.join(SECRETS_FOR_TESTING_ENV_PATH).to_string_lossy().to_string());

    cli_args
}

#[tokio::test]
#[traced_test]
async fn test_config_manager_runner_with_dummy_config() {
    // Use all the consolidated test config files
    let cli_args = create_cli_args_from_deployment_system();
    let mock_client = MockConfigManagerClient::new();
    let shared_client: Arc<dyn ConfigManagerClient> = Arc::new(mock_client);
    let runner = ConfigManagerRunner::new(shared_client, cli_args);
    // Test the update_consensus_config method directly
    let result = runner.update_consensus_config().await;
    // Verify the result.
    assert!(result.is_ok(), "update_consensus_config should succeed");
    assert!(logs_contain("Loading and validating config"));
    assert!(logs_contain("Built consensus dynamic config"));
    assert!(logs_contain("Would send consensus dynamic config.validator_id:"));
}

#[tokio::test]
#[traced_test]
async fn test_config_manager_runner_validates_consensus_dynamic_config_updates() {
    let workspace_root = resolve_project_relative_path(".").expect("Failed to get project root");

    // Create temporary copies of the consolidated config files we can modify
    let original_consolidated = workspace_root
        .join("crates/apollo_deployments/resources/deployments/testing/consolidated.json");
    let temp_consolidated = NamedTempFile::new().expect("Failed to create temp consolidated file");

    let original_content = fs::read_to_string(&original_consolidated)
        .expect("Failed to read original consolidated.json");

    fs::write(temp_consolidated.path(), &original_content)
        .expect("Failed to write temp consolidated file");

    let mut cli_args = create_cli_args_from_deployment_system();
    // Replace the consolidated.json path with our temp file
    for arg in &mut cli_args {
        if arg.contains("consolidated.json") {
            *arg = temp_consolidated.path().to_string_lossy().to_string();
            break;
        }
    }

    let mock_client = MockConfigManagerClient::new();
    let shared_client: Arc<dyn ConfigManagerClient> = Arc::new(mock_client);
    let runner = ConfigManagerRunner::new(shared_client, cli_args);

    // Load initial config
    let result1 = runner.update_consensus_config().await;
    assert!(result1.is_ok(), "First config load should succeed");

    // Verify initial config was processed
    assert!(logs_contain("Built consensus dynamic config"));
    assert!(
        logs_contain(
            "Would send consensus dynamic config.validator_id: \
             0x0000000000000000000000000000000000000000000000000000000000000064 to config manager"
        ),
        "Should contain default validator_id 0x64"
    );

    let modified_config = json!({
        "validator_id": "0x999"
    });

    // Write modified config to the temp file
    fs::write(temp_consolidated.path(), modified_config.to_string())
        .expect("Failed to write modified consolidated config");

    // Load config again with same cli args - should detect changes
    let result2 = runner.update_consensus_config().await;
    assert!(result2.is_ok(), "Second config load should succeed");

    // Validate that changes were detected and processed
    assert!(logs_contain("Built consensus dynamic config"));
    assert!(
        logs_contain(
            "Would send consensus dynamic config.validator_id: \
             0x0000000000000000000000000000000000000000000000000000000000000999 to config manager"
        ),
        "Should contain modified validator_id 0x999"
    );

    println!("Config update test completed successfully");
}
