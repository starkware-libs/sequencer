use std::collections::HashSet;
use std::env;

use apollo_config::CONFIG_FILE_ARG;
use apollo_infra_utils::dumping::{serialize_to_file, serialize_to_file_test};
use apollo_infra_utils::path::resolve_project_relative_path;
use apollo_node::config::component_execution_config::{
    ActiveComponentExecutionMode,
    ReactiveComponentExecutionMode,
};
use apollo_node::config::node_config::SequencerNodeConfig;
use apollo_node::config::test_utils::private_parameters;
use serde_json::to_value;
use strum::IntoEnumIterator;
use tempfile::NamedTempFile;

use crate::deployment_definitions::DEPLOYMENTS;
use crate::service::NodeType;
use crate::test_utils::{SecretsConfigOverride, FIX_BINARY_NAME};

/// Test that the deployment file is up to date.
#[test]
fn deployment_files_are_up_to_date() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");

    for node_type in NodeType::iter() {
        node_type.test_dump_service_component_configs(None);
    }
    for deployment in DEPLOYMENTS.iter().flat_map(|f| f()) {
        serialize_to_file_test(
            &deployment,
            deployment.deployment_file_path().to_str().unwrap(),
            FIX_BINARY_NAME,
        );
        deployment.test_dump_config_override_files();
    }
}

// Test that each service config files constitute a valid config.
#[test]
fn load_and_process_service_config_files() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");

    // Create a dummy secrets value to the config file paths.
    let temp_file = NamedTempFile::new().unwrap();
    let temp_file_path = temp_file.path().to_str().unwrap();
    let secrets_config_override = SecretsConfigOverride::default();
    serialize_to_file(to_value(&secrets_config_override).unwrap(), temp_file_path);

    for deployment in DEPLOYMENTS.iter().flat_map(|f| f()) {
        for mut service_config_paths in deployment.get_config_file_paths().into_iter() {
            println!(
                "Loading deployment {} in path {:?} with application files {:?} ... ",
                deployment.get_node_type(),
                deployment.deployment_file_path(),
                service_config_paths
            );

            // Add the secrets config file path to the config load command.
            service_config_paths.push(temp_file_path.to_string());

            let config_file_args: Vec<String> = service_config_paths
                .clone()
                .into_iter()
                .flat_map(|path| vec![CONFIG_FILE_ARG.to_string(), path])
                .collect();

            let mut config_load_command: Vec<String> = vec!["command_name_placeholder".to_string()];
            config_load_command.extend(config_file_args);
            let load_result = SequencerNodeConfig::load_and_process(config_load_command);

            load_result.unwrap_or_else(|err| {
                panic!(
                    "Loading deployment in path {:?} with application config files {:?}\nResulted \
                     in error: {}",
                    deployment.deployment_file_path(),
                    service_config_paths,
                    err
                );
            });
        }
    }
}

/// Test that the private values in the apollo node config schema match the secrets config override
/// schema.
#[test]
fn secrets_config_and_private_parameters_config_schema_compatibility() {
    let secrets_config_override = SecretsConfigOverride::default();
    let secrets_provided_by_config = to_value(&secrets_config_override)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<HashSet<_>>();
    let secrets_required_by_schema = private_parameters();

    let only_in_config: HashSet<_> =
        secrets_provided_by_config.difference(&secrets_required_by_schema).collect();
    let only_in_schema: HashSet<_> =
        secrets_required_by_schema.difference(&secrets_provided_by_config).collect();

    if !(only_in_config.is_empty() && only_in_schema.is_empty()) {
        panic!(
            "Secrets config override schema mismatch:\nSecrets provided by config: \
             {secrets_provided_by_config:?}\nSecrets required by schema: \
             {secrets_required_by_schema:?}\nOnly in config: {only_in_config:?}\nOnly in schema: \
             {only_in_schema:?}"
        );
    }
}

#[test]
fn l1_components_state_consistency() {
    for deployment in DEPLOYMENTS.iter().flat_map(|f| f()) {
        let node_type = deployment.get_node_type();
        let component_configs = node_type.get_component_configs(None);

        let l1_gas_price_provider_indicator = component_configs.values().any(|component_config| {
            component_config.l1_gas_price_provider.execution_mode
                != ReactiveComponentExecutionMode::Disabled
        });
        let l1_provider_indicator = component_configs.values().any(|component_config| {
            component_config.l1_provider.execution_mode != ReactiveComponentExecutionMode::Disabled
        });

        let l1_gas_price_scraper_indicator = component_configs.values().any(|component_config| {
            component_config.l1_gas_price_scraper.execution_mode
                != ActiveComponentExecutionMode::Disabled
        });
        let l1_scraper_indicator = component_configs.values().any(|component_config| {
            component_config.l1_scraper.execution_mode != ActiveComponentExecutionMode::Disabled
        });

        assert_eq!(
            l1_gas_price_provider_indicator, l1_gas_price_scraper_indicator,
            "L1 gas price provider and scraper should either be both enabled or both disabled."
        );
        assert_eq!(
            l1_provider_indicator, l1_scraper_indicator,
            "L1 provider and scraper should either be both enabled or both disabled."
        );
    }
}
