//! presentation of a configuration, with hiding or exposing private parameters.

use std::collections::BTreeSet;
use std::ops::IndexMut;

use itertools::Itertools;
use serde::Serialize;

use crate::{ConfigError, ParamPath};

/// Returns presentation of the public parameters in the config.
///
/// When `include_private_parameters` is `false`, every path in `private_paths` is redacted from the
/// presentation (the dump-independent redaction mechanism). `private_paths` is the set of `Private`
/// param paths for `config`'s type, injected by the caller (this crate sits below the crate that
/// owns the privacy registry, so it cannot derive the set itself).
pub fn get_config_presentation<T: Serialize>(
    config: &T,
    include_private_parameters: bool,
    private_paths: &BTreeSet<ParamPath>,
) -> Result<serde_json::Value, ConfigError> {
    let mut config_presentation = serde_json::to_value(config)?;
    if include_private_parameters {
        return Ok(config_presentation);
    }

    // Remove every private param path from the nested config presentation.
    for param_path in private_paths {
        remove_path_from_json(param_path, &mut config_presentation)?;
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
