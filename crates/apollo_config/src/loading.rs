//! Loads a configuration object, and set values for the fields in the following order of priority:
//! * Command line arguments.
//! * Environment variables (capital letters).
//! * Custom config files, separated by ',' (comma), from last to first.
//! * Default config file.

use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::ops::IndexMut;
use std::path::PathBuf;

use clap::parser::Values;
use clap::Command;
use command::{get_command_matches, update_config_map_by_command_args};
use itertools::any;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use tracing::{info, instrument};

use crate::dumping::ConfigPointers;
use crate::validators::validate_path_exists;
use crate::{
    command,
    ConfigError,
    ParamPath,
    SerializationType,
    SerializedContent,
    SerializedParam,
    CONFIG_FILE_ARG_NAME,
    FIELD_SEPARATOR,
    IS_NONE_MARK,
};

/// Deserializes config from flatten JSON.
/// For an explanation of `for<'a> Deserialize<'a>` see
/// `<https://doc.rust-lang.org/nomicon/hrtb.html>`.
#[instrument(skip(config_map))]
pub fn load<T: for<'a> Deserialize<'a>>(
    config_map: &BTreeMap<ParamPath, Value>,
) -> Result<T, ConfigError> {
    let mut nested_map = json!({});
    for (param_path, value) in config_map {
        let mut entry = &mut nested_map;
        for config_name in param_path.split('.') {
            entry = entry.index_mut(config_name);
        }
        *entry = value.clone();
    }
    Ok(serde_json::from_value(nested_map)?)
}

/// Deserializes a json config file, updates the values by the given arguments for the command, and
/// set values for the pointers.
pub fn load_and_process_config<T: for<'a> Deserialize<'a>>(
    config_schema_file: File,
    command: Command,
    args: Vec<String>,
    ignore_default_values: bool,
) -> Result<T, ConfigError> {
    let deserialized_config_schema: Map<ParamPath, Value> =
        serde_json::from_reader(&config_schema_file)?;
    // Store the pointers separately from the default values. The pointers will receive a value
    // only at the end of the process.
    let (config_map, pointers_map) = split_pointers_map(deserialized_config_schema.clone());
    // Take param paths with corresponding descriptions, and get the matching arguments.
    let mut arg_matches = get_command_matches(&config_map, command, args)?;
    // Retaining values from the default config map for backward compatibility.
    let (mut values_map, types_map) = split_values_and_types(config_map);
    if ignore_default_values {
        info!("Ignoring default values by overriding with an empty map.");
        values_map = BTreeMap::new();
    }
    // If the config_file arg is given, updates the values map according to this files.
    if let Some(custom_config_paths) = arg_matches.remove_many::<PathBuf>(CONFIG_FILE_ARG_NAME) {
        update_config_map_by_custom_configs(&mut values_map, &types_map, custom_config_paths)?;
    };
    // Updates the values map according to the args.
    update_config_map_by_command_args(&mut values_map, &types_map, &arg_matches)?;
    // Set values to the pointers.
    update_config_map_by_pointers(&mut values_map, &pointers_map)?;
    // Set values according to the is-none marks.
    update_optional_values(&mut values_map);
    // Build and return a Config object.
    let load_result = load(&values_map);

    if load_result.is_err() {
        let input_keys = values_map.keys().cloned().collect::<HashSet<_>>();

        // TODO address pointers in the config schema.
        let mut schema_keys = deserialized_config_schema.keys().cloned().collect::<HashSet<_>>();

        let optional_params =
            get_optional_params(&deserialized_config_schema.keys().cloned().collect::<Vec<_>>());
        let optional_params_set: HashSet<String> = optional_params.iter().cloned().collect();
        // let none_params = extract_none_params_and_remove_optional_keys(&mut config_map,
        // optional_params); remove_none_params(&mut config_map, &none_params);
        // let set: HashSet<String> = optional_params.iter().map(|s| s.as_str()).collect();
        // schema_keys = schema_keys.difference(&optional_params_set).cloned().collect::<HashSet<_>>();

        let only_in_input: HashSet<_> = input_keys.difference(&schema_keys).collect();
        let only_in_schema: HashSet<_> = schema_keys.difference(&input_keys).collect();

        if !(only_in_input.is_empty() && only_in_schema.is_empty()) {
            // TODO edit msg
            panic!(
                "Schema-values mismatch:\nOnly in config: {:#?}\nOnly in schema: {:#?}",
                only_in_input, only_in_schema
            );
        }

        // load all keys from the config schema
        // load all pointers from the pointers map
        // remove all pointing params from schema, and add the pointer target params to the expected
        // keys

        // let schema_keys = &config_map.keys().collect::<Vec<_>>();

        info!("Failed to load config with values: {:#?}", values_map);
    }

    load_result
}

// Separates a json map into config map of the raw values and pointers map.
pub(crate) fn split_pointers_map(
    json_map: Map<String, Value>,
) -> (BTreeMap<ParamPath, SerializedParam>, BTreeMap<ParamPath, ParamPath>) {
    let mut config_map: BTreeMap<String, SerializedParam> = BTreeMap::new();
    let mut pointers_map: BTreeMap<ParamPath, ParamPath> = BTreeMap::new();
    for (param_path, stored_param) in json_map {
        let Ok(ser_param) = serde_json::from_value::<SerializedParam>(stored_param.clone()) else {
            unreachable!("Invalid type in the json config map")
        };
        match ser_param.content {
            SerializedContent::PointerTarget(pointer_target) => {
                pointers_map.insert(param_path, pointer_target);
            }
            _ => {
                config_map.insert(param_path, ser_param);
            }
        };
    }
    (config_map, pointers_map)
}

// Removes the description from the config map, and splits the config map into default values and
// types of the default and required values.
// The types map includes required params, that do not have a value yet.
pub(crate) fn split_values_and_types(
    config_map: BTreeMap<ParamPath, SerializedParam>,
) -> (BTreeMap<ParamPath, Value>, BTreeMap<ParamPath, SerializationType>) {
    let mut values_map: BTreeMap<ParamPath, Value> = BTreeMap::new();
    let mut types_map: BTreeMap<ParamPath, SerializationType> = BTreeMap::new();
    for (param_path, serialized_param) in config_map {
        let Some(serialization_type) = serialized_param.content.get_serialization_type() else {
            continue;
        };
        types_map.insert(param_path.clone(), serialization_type);

        if let SerializedContent::DefaultValue(value) = serialized_param.content {
            values_map.insert(param_path, value);
        };
    }
    (values_map, types_map)
}

// Updates the config map by param path to value custom json files.
pub(crate) fn update_config_map_by_custom_configs(
    config_map: &mut BTreeMap<ParamPath, Value>,
    types_map: &BTreeMap<ParamPath, SerializationType>,
    custom_config_paths: Values<PathBuf>,
) -> Result<(), ConfigError> {
    for config_path in custom_config_paths {
        info!("Loading custom config file: {:?}", config_path);
        validate_path_exists(&config_path)?;
        let file = std::fs::File::open(config_path)?;
        let custom_config: Map<String, Value> = serde_json::from_reader(file)?;
        for (param_path, json_value) in custom_config {
            update_config_map(config_map, types_map, param_path.as_str(), json_value)?;
        }
    }
    Ok(())
}

// Sets values in the config map to the params in the pointers map.
pub(crate) fn update_config_map_by_pointers(
    config_map: &mut BTreeMap<ParamPath, Value>,
    pointers_map: &BTreeMap<ParamPath, ParamPath>,
) -> Result<(), ConfigError> {
    for (param_path, target_param_path) in pointers_map {
        let Some(target_value) = config_map.get(target_param_path) else {
            return Err(ConfigError::PointerTargetNotFound {
                target_param: target_param_path.to_owned(),
            });
        };
        config_map.insert(param_path.to_owned(), target_value.clone());
    }
    Ok(())
}

fn get_optional_params(config_map_keys: &[ParamPath]) -> Vec<ParamPath> {
    config_map_keys
        .iter()
        .filter_map(|param_path| {
            if param_path.ends_with(&format!(".{IS_NONE_MARK}")) {
                param_path.strip_suffix(&format!(".{IS_NONE_MARK}")).map(|s| s.to_owned())
            } else {
                None
            }
        })
        .collect()
}

fn extract_none_params_and_remove_optional_keys(
    config_map: &mut BTreeMap<ParamPath, Value>,
    optional_params: Vec<ParamPath>,
) -> Vec<ParamPath> {
    let mut none_params = vec![];

    for optional_param in optional_params {
        let key = format!("{optional_param}.{IS_NONE_MARK}");
        let optional_param_value = config_map.remove(&key).expect("Not found optional param");

        if optional_param_value == json!(true) {
            none_params.push(optional_param);
        }
    }

    none_params
}

// Remove param paths that start with any None param, and set the outer-most param to be null.
fn remove_none_params(config_map: &mut BTreeMap<ParamPath, Value>, none_params: &[ParamPath]) {
    // Remove param paths that start with any None param.

    config_map.retain(|param_path, _| {
        !any(none_params, |none_param| {
            param_path.starts_with(format!("{none_param}{FIELD_SEPARATOR}").as_str())
                || param_path == none_param
        })
    });

    // Set null for the None params.
    for none_param in none_params {
        let mut is_nested_in_outer_none_config = false;
        for other_none_param in none_params {
            if none_param.starts_with(other_none_param) && none_param != other_none_param {
                is_nested_in_outer_none_config = true;
            }
        }
        if is_nested_in_outer_none_config {
            continue;
        }
        config_map.insert(none_param.clone(), Value::Null);
    }
}

// Removes the none marks, and sets null for the params marked as None instead of the inner params.
pub(crate) fn update_optional_values(config_map: &mut BTreeMap<ParamPath, Value>) {
    let keys_vec: Vec<_> = config_map.keys().cloned().collect();
    let optional_params = get_optional_params(&keys_vec);
    let none_params = extract_none_params_and_remove_optional_keys(config_map, optional_params);
    remove_none_params(config_map, &none_params);
}

pub(crate) fn update_config_map(
    config_map: &mut BTreeMap<ParamPath, Value>,
    types_map: &BTreeMap<ParamPath, SerializationType>,
    param_path: &str,
    new_value: Value,
) -> Result<(), ConfigError> {
    let Some(serialization_type) = types_map.get(param_path) else {
        return Err(ConfigError::ParamNotFound { param_path: param_path.to_string() });
    };
    let is_type_matched = match serialization_type {
        SerializationType::Boolean => new_value.is_boolean(),
        SerializationType::Float => new_value.is_number(),
        SerializationType::NegativeInteger => new_value.is_number(),
        SerializationType::PositiveInteger => new_value.is_number(),
        SerializationType::String => new_value.is_string(),
    };
    if !is_type_matched {
        return Err(ConfigError::ChangeRequiredParamType {
            param_path: param_path.to_string(),
            required: serialization_type.to_owned(),
            given: new_value,
        });
    }

    config_map.insert(param_path.to_owned(), new_value);
    Ok(())
}
