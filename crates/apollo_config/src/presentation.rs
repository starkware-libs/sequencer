//! presentation of a configuration, with hiding or exposing private parameters.

use std::ops::IndexMut;

use itertools::Itertools;
use serde::Serialize;

use crate::ConfigError;

/// Returns a presentation of the config with private fields optionally removed.
///
/// `private_paths`: dotted-path strings for fields to strip when `include_private` is false.
pub fn get_config_presentation<T: Serialize>(
    config: &T,
    private_paths: &[&str],
    include_private: bool,
) -> Result<serde_json::Value, ConfigError> {
    let mut config_presentation = serde_json::to_value(config)?;
    if include_private {
        return Ok(config_presentation);
    }
    for path in private_paths {
        remove_path_from_json(path, &mut config_presentation)?;
    }
    Ok(config_presentation)
}

// Gets a json in the format:
// {
//      a: {
//          b: {
//              v1: 1,
//              v2: 2
//          }
//      }
// }
// and a param path, for example 'a.b.v1', and removes the v1 from the json if it exists.
// The result will be:
// {
//      a: {
//          b: {
//              v2: 2
//          }
//      }
// }
// If path not found in json then do nothing.
fn remove_path_from_json(
    param_path: &str,
    json: &mut serde_json::Value,
) -> Result<(), ConfigError> {
    // given param_path = "a.b.v1", path_to_entry will be ["a", "b"] and entry_to_remove will
    // be "v1".
    let mut path_to_entry = param_path.split('.').collect_vec();
    let Some(entry_to_remove) = path_to_entry.pop() else {
        // TODO(Yair): Can we expect this to never happen?
        return Ok(()); // Empty param path.
    };
    let mut inner_json = json;
    for path in &path_to_entry {
        if !inner_json.is_object() {
            return Ok(()); // Path not found in json.
        }
        inner_json = inner_json.index_mut(path);
    }
    // Remove entry_to_remove from inner_json
    if let Some(obj) = inner_json.as_object_mut() {
        obj.remove(entry_to_remove);
    }
    Ok(())
}
