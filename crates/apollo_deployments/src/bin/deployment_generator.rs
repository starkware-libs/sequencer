use apollo_deployments::deployment_definitions::DEPLOYMENTS;
use apollo_deployments::service::NodeType;
use apollo_infra_utils::dumping::serialize_to_file;
use strum::IntoEnumIterator;

/// Creates the deployment json file.
fn main() {
    for node_type in NodeType::iter() {
        node_type.dump_service_component_configs(None);
    }
    for deployment in DEPLOYMENTS.iter().flat_map(|f| f()) {
        serialize_to_file(&deployment, deployment.deployment_file_path().to_str().unwrap());
        deployment.dump_config_override_files();
    }
}
