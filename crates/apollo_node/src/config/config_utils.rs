use std::collections::{BTreeSet, HashSet};
use std::fs::File;
use std::path::Path;

use apollo_config::dumping::{combine_config_map_and_pointers, Pointers, SerializeConfig};
use apollo_config::{ParamPath, SerializedParam};
use apollo_infra_utils::dumping::serialize_to_file;
use apollo_infra_utils::path::resolve_project_relative_path;
use serde_json::{Map, Value};
use tracing::error;
use validator::ValidationError;

use crate::config::definitions::ConfigPointersMap;
use crate::config::node_config::{
    SequencerNodeConfig,
    CONFIG_POINTERS,
    CONFIG_SCHEMA_PATH,
    POINTER_TARGET_VALUE,
};

/// Returns the set of all non-pointer private parameters and all pointer target parameters pointed
/// by private parameters.
pub fn private_parameters() -> BTreeSet<ParamPath> {
    let config_file_name = &resolve_project_relative_path(CONFIG_SCHEMA_PATH).unwrap();
    let config_schema_file = File::open(config_file_name).unwrap();
    let deserialized_config_schema: Map<ParamPath, Value> =
        serde_json::from_reader(config_schema_file).unwrap();

    let mut private_values = BTreeSet::new();
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

pub(crate) fn create_validation_error(
    error_msg: String,
    validate_code: &'static str,
    validate_error_msg: &'static str,
) -> ValidationError {
    error!(error_msg);
    let mut error = ValidationError::new(validate_code);
    error.message = Some(validate_error_msg.into());
    error
}
/// Transforms a nested JSON dictionary object into a simplified JSON dictionary object by
/// extracting specific values from the inner dictionaries.
///
/// # Parameters
/// - `config_map`: A reference to a `serde_json::Value` that must be a JSON dictionary object. Each
///   key in the object maps to another JSON dictionary object.
///
/// # Returns
/// - A `serde_json::Value` dictionary object where:
///   - Each key is preserved from the top-level dictionary.
///   - Each value corresponds to the `"value"` field of the nested JSON dictionary under the
///     original key.
///
/// # Panics
/// This function panics if the provided `config_map` is not a JSON dictionary object.
pub fn config_to_preset(config_map: &Value) -> Value {
    // Ensure the config_map is a JSON object.
    if let Value::Object(map) = config_map {
        let mut result = Map::new();

        for (key, value) in map {
            if let Value::Object(inner_map) = value {
                // Extract the value.
                if let Some(inner_value) = inner_map.get("value") {
                    // Add it to the result map
                    result.insert(key.clone(), inner_value.clone());
                }
            }
        }

        // Return the transformed result as a JSON object.
        Value::Object(result)
    } else {
        panic!("Config map is not a JSON object: {:?}", config_map);
    }
}

/// Keep "{prefix}.#is_none": true, remove all other "{prefix}.*" keys.
pub fn prune_by_is_none(v: Value) -> Value {
    let Value::Object(mut obj) = v else { return v };

    // Find prefixes whose is_none flag is true
    let mut prefixes: HashSet<String> = HashSet::new();
    for (k, val) in obj.iter() {
        if let Some(prefix) = k.strip_suffix(".#is_none") {
            if val.as_bool() == Some(true) {
                prefixes.insert(format!("{prefix}."));
            }
        }
    }

    // Remove keys that begin with any such prefix, except the "#is_none" flag itself
    obj.retain(|k, _| {
        if let Some(p) = prefixes.iter().find(|p| k.starts_with(&***p)) {
            // keep only the "{prefix}.#is_none" key
            k == &format!("{p}#is_none")
        } else {
            true
        }
    });

    Value::Object(obj)
}

// TODO(Nadin): Consider adding methods to ConfigPointers to encapsulate related functionality.
fn validate_all_pointer_targets_set(preset: Value) -> Result<(), ValidationError> {
    if let Some(preset_map) = preset.as_object() {
        for (key, value) in preset_map {
            if value == POINTER_TARGET_VALUE {
                return Err(create_validation_error(
                    format!("Pointer target not set for key: '{}'", key),
                    "pointer_target_not_set",
                    "Pointer target not set",
                ));
            }
        }
        Ok(())
    } else {
        Err(create_validation_error(
            "Preset must be an object".to_string(),
            "invalid_preset_format",
            "Preset is not a valid object",
        ))
    }
}

#[derive(Debug, Clone, Default)]
pub struct DeploymentBaseAppConfig {
    pub config: SequencerNodeConfig,
    config_pointers_map: ConfigPointersMap,
    non_pointer_params: Pointers,
}

impl DeploymentBaseAppConfig {
    pub fn new(
        config: SequencerNodeConfig,
        config_pointers_map: ConfigPointersMap,
        non_pointer_params: Pointers,
    ) -> Self {
        Self { config, config_pointers_map, non_pointer_params }
    }

    pub fn get_config(&self) -> &SequencerNodeConfig {
        &self.config
    }

    pub fn get_config_pointers_map(&self) -> &ConfigPointersMap {
        &self.config_pointers_map
    }

    pub fn modify_config<F>(&mut self, modify_config_fn: F)
    where
        F: Fn(&mut SequencerNodeConfig),
    {
        modify_config_fn(&mut self.config);
    }

    pub fn modify_config_pointers<F>(&mut self, modify_config_pointers_fn: F)
    where
        F: Fn(&mut ConfigPointersMap),
    {
        modify_config_pointers_fn(&mut self.config_pointers_map);
    }

    pub fn as_value(&self) -> Value {
        // Create the entire mapping of the config and the pointers, without the required params.
        let config_as_map = combine_config_map_and_pointers(
            self.config.dump(),
            // TODO(Tsabary): avoid the cloning here
            &self.config_pointers_map.clone().into(),
            &self.non_pointer_params,
        )
        .unwrap();

        // Extract only the required fields from the config map.
        let preset = config_to_preset(&config_as_map);
        validate_all_pointer_targets_set(preset.clone()).expect("Pointer target not set");
        preset
    }

    // TODO(Tsabary): unify path types throughout.
    pub fn dump_config_file(&self, config_path: &Path) {
        let value = self.as_value();
        serialize_to_file(
            value,
            config_path.to_str().expect("Should be able to convert path to string"),
        );
    }
}
