use std::env;

use apollo_deployments::service::NodeType;
use apollo_infra_utils::path::resolve_project_relative_path;
use strum::IntoEnumIterator;

/// Creates the deployment json file.
fn main() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");

    for node_type in NodeType::iter() {
        node_type.dump_service_component_configs(None);
        for node_service in node_type.all_service_names() {
            node_service.dump_node_service_replacer_app_config_files();
        }
    }
}
