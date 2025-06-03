use apollo_deployments::deployment_definitions::DEPLOYMENTS;
use apollo_infra_utils::dumping::serialize_to_file;

/// Creates the deployment json file.
fn main() {
    for deployment in DEPLOYMENTS.iter().flat_map(|f| f()) {
        serialize_to_file(&deployment, deployment.deployment_file_path().to_str().unwrap());
        deployment.dump_application_config_files();
    }
}
