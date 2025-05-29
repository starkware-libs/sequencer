use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::{value_parser, Arg, ArgMatches, Command};
use serde_json::{json, Value};

use crate::loading::update_config_map;
use crate::{ConfigError, ParamPath, SerializationType, SerializedParam, CONFIG_FILE_ARG_NAME};

pub(crate) fn get_command_matches(
    config_map: &BTreeMap<ParamPath, SerializedParam>,
    command: Command,
    command_input: Vec<String>,
) -> Result<ArgMatches, ConfigError> {
    Ok(command.args(build_args_parser(config_map)).try_get_matches_from(command_input)?)
}

// Takes matched arguments from the command line interface and env variables and updates the config
// map.
// Supports f64, u64, i64, bool and String.
pub(crate) fn update_config_map_by_command_args(
    config_map: &mut BTreeMap<ParamPath, Value>,
    types_map: &BTreeMap<ParamPath, SerializationType>,
    arg_match: &ArgMatches,
) -> Result<(), ConfigError> {
    for param_path_id in arg_match.ids() {
        let param_path = param_path_id.as_str();
        let new_value = get_arg_by_type(types_map, arg_match, param_path)?;
        update_config_map(config_map, types_map, param_path, new_value)?;
    }
    Ok(())
}

// Builds the parser for the command line flags and env variables according to the types of the
// values in the config map.
fn build_args_parser(config_map: &BTreeMap<ParamPath, SerializedParam>) -> Vec<Arg> {
    let mut args_parser = vec![
        // Custom_config_file_path.
        Arg::new(CONFIG_FILE_ARG_NAME)
            .long(CONFIG_FILE_ARG_NAME)
            .short('f')
            .value_delimiter(',')
            .help("Optionally sets a config file to use")
            .value_parser(value_parser!(PathBuf))
            .num_args(1..) // Allow multiple values
            .action(clap::ArgAction::Append), // Collect multiple occurrences
    ];

    for (param_path, serialized_param) in config_map.iter() {
        let Some(serialization_type) = serialized_param.content.get_serialization_type() else {
            continue; // Pointer target
        };
        let clap_parser = match serialization_type {
            SerializationType::Boolean => clap::value_parser!(bool),
            SerializationType::Float => clap::value_parser!(f64).into(),
            SerializationType::NegativeInteger => clap::value_parser!(i64).into(),
            SerializationType::PositiveInteger => clap::value_parser!(u64).into(),
            SerializationType::String => clap::value_parser!(String),
        };

        let arg = Arg::new(param_path)
            .long(param_path)
            .env(to_env_var_name(param_path))
            .help(&serialized_param.description)
            .value_parser(clap_parser)
            .allow_negative_numbers(true);

        args_parser.push(arg);
    }
    args_parser
}

// Converts clap arg_matches into json values.
fn get_arg_by_type(
    types_map: &BTreeMap<ParamPath, SerializationType>,
    arg_match: &ArgMatches,
    param_path: &str,
) -> Result<Value, ConfigError> {
    let serialization_type = types_map.get(param_path).expect("missing type");
    match serialization_type {
        SerializationType::Boolean => Ok(json!(arg_match.try_get_one::<bool>(param_path)?)),
        SerializationType::Float => Ok(json!(arg_match.try_get_one::<f64>(param_path)?)),
        SerializationType::NegativeInteger => Ok(json!(arg_match.try_get_one::<i64>(param_path)?)),
        SerializationType::PositiveInteger => Ok(json!(arg_match.try_get_one::<u64>(param_path)?)),
        SerializationType::String => Ok(json!(arg_match.try_get_one::<String>(param_path)?)),
    }
}

fn to_env_var_name(param_path: &str) -> String {
    param_path.replace("#is_none", "__is_none__").to_uppercase().replace('.', "__")
}
