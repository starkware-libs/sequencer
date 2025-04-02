use apollo_deployments::deployment_definitions::DEPLOYMENTS;
use apollo_infra_utils::dumping::serialize_to_file;

/// Creates the deployment json file.
fn main() {
    for deployment_fn in DEPLOYMENTS {
        let deployment_preset = deployment_fn();
        serialize_to_file(
            deployment_preset.get_deployment(),
            deployment_preset.get_dump_file_path(),
        );

        deployment_preset
            .get_deployment()
            .dump_application_config_files(deployment_preset.get_base_app_config_file_path());
    }
}
