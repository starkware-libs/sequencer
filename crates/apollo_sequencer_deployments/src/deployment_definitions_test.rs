use std::env;
use std::path::PathBuf;

use apollo_infra_utils::dumping::serialize_to_file_test;
use apollo_infra_utils::path::resolve_project_relative_path;

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

#[test]
fn application_config_files_exist() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");
    for deployment_fn in DEPLOYMENTS {
        let deployment_preset = deployment_fn();
        deployment_preset.get_deployment().assert_application_configs_exist();

        // TODO(Tsabary): the following dumps the application config instead of verifying the
        // already dumped values are updated. This is a temporary solution until the dump functions
        // will be rearranged.
        deployment_preset
            .get_deployment()
            .dump_application_config_files(deployment_preset.get_base_app_config_file_path());
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
