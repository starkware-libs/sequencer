use std::env;

use starknet_infra_utils::dumping::serialize_to_file_test;
use starknet_infra_utils::path::resolve_project_relative_path;

use crate::deployment::DeploymentAndPreset;
use crate::deployment_definitions::DEPLOYMENTS;

// TODO(Tsabary): bundle deployment and its preset path together, and create a list of all of these
// pairs. Then in the test, iterate over them and test each one.

/// Test that the deployment file is up to date. To update it run:
/// cargo run --bin deployment_generator -q
#[test]
fn deployment_files_are_up_to_date() {
    for deployment_fn in DEPLOYMENTS {
        let DeploymentAndPreset { deployment, dump_file_path } = deployment_fn();
        serialize_to_file_test(deployment, dump_file_path);
    }
}

#[test]
fn application_config_files_exist() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");
    for deployment_fn in DEPLOYMENTS {
        let DeploymentAndPreset { deployment, dump_file_path: _ } = deployment_fn();
        deployment.assert_application_configs_exist();
    }
}
