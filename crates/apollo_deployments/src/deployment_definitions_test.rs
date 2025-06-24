use std::env;

use apollo_config::CONFIG_FILE_ARG;
use apollo_infra_utils::dumping::serialize_to_file_test;
use apollo_infra_utils::path::resolve_project_relative_path;
use apollo_node::config::component_execution_config::{
    ActiveComponentExecutionMode,
    ReactiveComponentExecutionMode,
};
use apollo_node::config::node_config::SequencerNodeConfig;
use strum::IntoEnumIterator;

use crate::deployment::FIX_BINARY_NAME;
use crate::deployment_definitions::DEPLOYMENTS;
use crate::service::DeploymentName;

/// Test that the deployment file is up to date. To update it run:
/// cargo run --bin deployment_generator -q
#[test]
fn deployment_files_are_up_to_date() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");

    // TODO(Tsabary): The word "deployment" is overloaded. On one hand it means the "node
    // configuration" (e.g. hybrid), on the other it means the "k8s setups" (e.g. testing_env_2).
    // Need to fix that.
    for deployment_name in DeploymentName::iter() {
        deployment_name.test_dump_service_component_configs(None);
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
    for deployment in DEPLOYMENTS.iter().flat_map(|f| f()) {
        for service_config_paths in deployment.get_config_file_paths().into_iter() {
            println!(
                "Loading deployment {} in path {:?} with application files {:?} ... ",
                deployment.get_deployment_name(),
                deployment.deployment_file_path(),
                service_config_paths
            );

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

#[test]
fn l1_components_state_consistency() {
    for deployment in DEPLOYMENTS.iter().flat_map(|f| f()) {
        let deployment_name = deployment.get_deployment_name();
        let component_configs = deployment_name.get_component_configs(None);

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
