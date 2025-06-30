use std::collections::HashSet;
use std::fs::File;

use apollo_config::{ParamPath, SerializedParam};
use apollo_infra_utils::path::resolve_project_relative_path;
use serde_json::{Map, Value};

use crate::config::node_config::CONFIG_SCHEMA_PATH;

/// Returns a set of all private parameters non-pointer parameters.
pub fn private_parameters() -> HashSet<ParamPath> {
    let config_file_name = &resolve_project_relative_path(CONFIG_SCHEMA_PATH).unwrap();
    let config_schema_file = File::open(config_file_name).unwrap();
    let deserialized_config_schema: Map<ParamPath, Value> =
        serde_json::from_reader(config_schema_file).unwrap();

    let mut private_values = HashSet::new();
    for (param_path, stored_param) in deserialized_config_schema.into_iter() {
        let ser_param = serde_json::from_value::<SerializedParam>(stored_param).unwrap();
        if ser_param.is_private() {
            private_values.insert(param_path);
        }
    }
    private_values
}
