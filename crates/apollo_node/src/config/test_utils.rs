use std::collections::HashSet;
use std::fs::File;

use apollo_config::{ParamPath, SerializedParam};
use apollo_infra_utils::path::resolve_project_relative_path;
use serde_json::{Map, Value};

use crate::config::node_config::{CONFIG_POINTERS, CONFIG_SCHEMA_PATH};

/// Returns the set of all non-pointer private parameters and all pointer target parameters pointed
/// by private parameters.
pub fn private_parameters() -> HashSet<ParamPath> {
    let config_file_name = &resolve_project_relative_path(CONFIG_SCHEMA_PATH).unwrap();
    let config_schema_file = File::open(config_file_name).unwrap();
    let deserialized_config_schema: Map<ParamPath, Value> =
        serde_json::from_reader(config_schema_file).unwrap();

    let mut private_values = HashSet::new();
    for (param_path, stored_param) in deserialized_config_schema.into_iter() {
        let ser_param = serde_json::from_value::<SerializedParam>(stored_param).unwrap();
        // Find all private parameters.
        if ser_param.is_private() {
            let mut included_as_a_pointer = false;
            for ((pointer_target_param_path, _ser_param), pointing_params) in CONFIG_POINTERS.iter()
            {
                // If the parameter is a pointer, add its pointer target value.
                if pointing_params.contains(&param_path) {
                    private_values.insert(pointer_target_param_path.clone());
                    included_as_a_pointer = true;
                    continue;
                }
            }
            if !included_as_a_pointer {
                // If the parameter is not a pointer, add it directly.
                private_values.insert(param_path);
            }
        }
    }
    private_values
}
