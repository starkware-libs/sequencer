use std::env;
use std::path::PathBuf;

use apollo_infra_utils::dumping::serialize_to_file_test;
use apollo_infra_utils::path::resolve_project_relative_path;
use apollo_node::config::component_execution_config::{
    ActiveComponentExecutionMode,
    ReactiveComponentExecutionMode,
};
use apollo_node::config::node_config::SequencerNodeConfig;

use crate::deployment_definitions::{deployment_file_path, Environment, DEPLOYMENTS};

/// Test that the deployment file is up to date. To update it run:
/// cargo run --bin deployment_generator -q
#[test]
fn deployment_files_are_up_to_date() {
    for deployment_fn in DEPLOYMENTS {
        let deployment_preset = deployment_fn();
        serialize_to_file_test(
            deployment_preset.get_deployment(),
            deployment_preset.get_dump_file_path().to_str().unwrap(),
        );

        // TODO(Tsabary): test that the dumped app-config files are up to date, i.e., their current
        // content matches the dumped on. This test will replace the application_config_files_exist
        // test below.
    }
}

// Test the base application config files are successfully loaded and processed.
// TODO(Tsabary): consider having a similar test for the dumped (non-base) application config files.
#[test]
fn load_and_process_base_application_config_files_schema() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");
    for deployment_fn in DEPLOYMENTS {
        let deployment_preset = deployment_fn();
        // TODO(Tsabary): "--config_file" should be a constant.
        let load_result = SequencerNodeConfig::load_and_process(vec![
            "command_name_placeholder".to_string(),
            "--config_file".to_string(),
            deployment_preset.get_base_app_config_file_path().to_string(),
        ]);
        println!("{:?}", load_result);
        assert!(load_result.is_ok());
    }
}

#[test]
fn application_config_files_exist() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");
    for deployment_fn in DEPLOYMENTS {
        let deployment_preset = deployment_fn();
        deployment_preset.get_deployment().assert_application_configs_exist();

        deployment_preset
            .get_deployment()
            .test_dump_application_config_files(deployment_preset.get_base_app_config_file_path());
    }
}

// TODO(Tsabary): add a sanity test that the chain id matches the value in the config.

#[test]
fn l1_components_state_consistency() {
    for deployment_fn in DEPLOYMENTS {
        let deployment_preset = deployment_fn();
        let deployment_name = deployment_preset.get_deployment().get_deployment_name();
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

// The point of this test is to serve as a reminder to update all relevant paths in all relevant
// paths when changing the deployment file path.
#[test]
fn deployment_config_paths() {
    assert_eq!(
        deployment_file_path(Environment::Testing, "test_deployment"),
        PathBuf::from("config/sequencer/testing/deployment_configs/test_deployment.json")
    );

    assert_eq!(
        deployment_file_path(Environment::SepoliaIntegration, "test_deployment"),
        PathBuf::from(
            "config/sequencer/sepolia_integration/deployment_configs/test_deployment.json"
        )
    );

    assert_eq!(
        deployment_file_path(Environment::SepoliaTestnet, "test_deployment"),
        PathBuf::from("config/sequencer/sepolia_testnet/deployment_configs/test_deployment.json")
    );

    assert_eq!(
        deployment_file_path(Environment::Mainnet, "test_deployment"),
        PathBuf::from("config/sequencer/mainnet/deployment_configs/test_deployment.json")
    );
}
