//! Loads a configuration object from native config files: a nested base config followed by flat
//! dotted-key secret overrides.

use std::collections::BTreeMap;
use std::fs::File;
use std::ops::IndexMut;
use std::path::PathBuf;

use clap::Command;
use command::get_command_matches;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use tracing::{debug, info, instrument};

use crate::validators::validate_path_exists;
use crate::{command, ConfigError, ParamPath, CONFIG_FILE_ARG_NAME, FIELD_SEPARATOR};

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

/// Deserializes the config directly from nested JSON files.
///
/// The first path is the base config: a nested JSON object matching the target struct's field
/// hierarchy. Each subsequent path is a flat dotted-key secret file whose entries are written into
/// the nested base before deserialization. Unlike the preset path, no pointers, env/CLI overrides,
/// or `#is_none` marks are applied — the base file is expected to be a complete, resolved config.
fn load_native<T: for<'a> Deserialize<'a>>(
    custom_config_paths: Vec<PathBuf>,
) -> Result<T, ConfigError> {
    let [base_config_path, secret_config_path] =
        custom_config_paths.as_array().ok_or(ConfigError::NativeModeRequiresTwoConfigFiles)?;

    validate_path_exists(base_config_path)?;
    info!("Loading native config file: {:?}", base_config_path);
    let mut nested_map: Value = serde_json::from_reader(File::open(base_config_path)?)?;
    validate_path_exists(secret_config_path)?;
    info!("Loading native secret config file: {:?}", secret_config_path);

    let secret_config: Map<String, Value> =
        serde_json::from_reader(File::open(secret_config_path)?)?;
    for (param_path, value) in secret_config {
        set_nested_value(&mut nested_map, &param_path, value);
    }

    Ok(serde_json::from_value(nested_map)?)
}

/// Writes `value` into `nested_map` at the leaf named by the dotted `param_path`, descending
/// through (and preserving the siblings of) the intermediate objects. The leaf key itself is
/// created if absent.
///
/// Traversal stops without writing anything if an intermediate on the path is `null` (a `None`
/// optional, e.g. a disabled component) or is missing/not an object. The base config is the source
/// of truth for which subtrees exist; a secret aimed at a subtree the base omits is irrelevant, and
/// vivifying it would produce a partial object that fails deserialization. This also avoids any
/// panic on a type-mismatched intermediate.
fn set_nested_value(nested_map: &mut Value, param_path: &str, value: Value) {
    let mut segments = param_path.split(FIELD_SEPARATOR).peekable();
    let mut entry = nested_map;
    while let Some(segment) = segments.next() {
        let Value::Object(map) = entry else {
            return;
        };
        if segments.peek().is_none() {
            map.insert(segment.to_owned(), value);
            return;
        }
        // Descend into the intermediate; stop if it is absent or an explicit `null` (`None`).
        match map.get_mut(segment) {
            Some(child) if !child.is_null() => entry = child,
            _ => {
                debug!(
                    "Skipping secret override {param_path:?}: intermediate {segment:?} is absent \
                     or None in the base config."
                );
                return;
            }
        }
    }
}

/// Loads a config from native config files.
///
/// Reads the `--config_file` arguments and deserializes the config directly from them via
/// [`load_native`]: the first file is the nested base config and each subsequent file is a flat
/// dotted-key secret override. `ignore_default_values` is retained for call-site compatibility but
/// has no effect on the native path (the base file is already a complete, resolved config).
pub fn load_and_process_config<T: for<'a> Deserialize<'a>>(
    command: Command,
    args: Vec<String>,
    _ignore_default_values: bool,
) -> Result<T, ConfigError> {
    let mut arg_matches = get_command_matches(command, args)?;
    let custom_config_paths: Vec<PathBuf> = arg_matches
        .remove_many::<PathBuf>(CONFIG_FILE_ARG_NAME)
        .map(|paths| paths.collect())
        .unwrap_or_default();
    load_native(custom_config_paths)
}
