use std::env;

use starknet_infra_utils::dumping::serialize_to_file_test;
use starknet_infra_utils::path::resolve_project_relative_path;

use crate::deployment_definitions::DEPLOYMENTS;

/// Test that the deployment file is up to date. To update it run:
/// cargo run --bin deployment_generator -q
#[test]
fn deployment_files_are_up_to_date() {
    for deployment_fn in DEPLOYMENTS {
        let deployment_preset = deployment_fn();
        serialize_to_file_test(
            deployment_preset.get_deployment(),
            deployment_preset.get_dump_file_path(),
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
