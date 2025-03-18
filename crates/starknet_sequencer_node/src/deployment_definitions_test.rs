use std::env;

use starknet_infra_utils::dumping::serialize_to_file_test;
use starknet_infra_utils::path::resolve_project_relative_path;

use crate::deployment_definitions::{
    create_main_deployment,
    create_testing_deployment,
    MAIN_DEPLOYMENT_PRESET_PATH,
    TESTING_DEPLOYMENT_PRESET_PATH,
};

// TODO(Tsabary): bundle deployment and its preset path together, and create a list of all of these
// pairs. Then in the test, iterate over them and test each one.

/// Test that the deployment file is up to date. To update it run:
/// cargo run --bin deployment_generator -q
#[test]
fn deployment_files_are_up_to_date() {
    serialize_to_file_test(create_main_deployment(), MAIN_DEPLOYMENT_PRESET_PATH);
    serialize_to_file_test(create_testing_deployment(), TESTING_DEPLOYMENT_PRESET_PATH);
}

#[test]
fn application_config_files_exist() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");
    for deployment in &[create_main_deployment(), create_testing_deployment()] {
        deployment.assert_application_configs_exist();
    }
}
