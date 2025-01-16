use std::path::PathBuf;

use papyrus_config::dumping::{combine_config_map_and_pointers, SerializeConfig};
use serde_json::Value;
use starknet_sequencer_node::config::config_utils::{
    config_to_preset,
    dump_json_data,
    RequiredParams,
};
use starknet_sequencer_node::config::node_config::{
    SequencerNodeConfig,
    CONFIG_NON_POINTERS_WHITELIST,
    CONFIG_POINTERS,
};

// TODO(Tsabary): Move here all config-related functions from "integration_test_utils.rs".

const NODE_CONFIG_CHANGES_FILE_PATH: &str = "node_integration_test_config_changes.json";

/// Creates a config file for the sequencer node for an integration test.
pub(crate) fn dump_config_file_changes(
    config: &SequencerNodeConfig,
    required_params: RequiredParams,
    dir: PathBuf,
) -> PathBuf {
    // Create the entire mapping of the config and the pointers, without the required params.
    let config_as_map = combine_config_map_and_pointers(
        config.dump(),
        &CONFIG_POINTERS,
        &CONFIG_NON_POINTERS_WHITELIST,
    )
    .unwrap();

    // Extract only the required fields from the config map.
    let mut preset = config_to_preset(&config_as_map);

    // Add the required params to the preset.
    add_required_params_to_preset(&mut preset, required_params.as_json());

    // Dump the preset to a file, return its path.
    let node_config_path = dump_json_data(preset, NODE_CONFIG_CHANGES_FILE_PATH, dir);
    assert!(node_config_path.exists(), "File does not exist: {:?}", node_config_path);
    node_config_path
}

/// Merges required parameters into an existing preset JSON object.
///
/// # Parameters
/// - `preset`: A mutable reference to a `serde_json::Value` representing the preset. It must be a
///   JSON dictionary object where additional parameters will be added.
/// - `required_params`: A reference to a `serde_json::Value` representing the required parameters.
///   It must also be a JSON dictionary object. Its keys and values will be merged into the
///   `preset`.
///
/// # Behavior
/// - For each key-value pair in `required_params`, the pair is inserted into `preset`.
/// - If a key already exists in `preset`, its value will be overwritten by the value from
///   `required_params`.
/// - Both `preset` and `required_params` must be JSON dictionary objects; otherwise, the function
///   panics.
///
/// # Panics
/// This function panics if either `preset` or `required_params` is not a JSON dictionary object, or
/// if the `preset` already contains a key from the `required_params`.
fn add_required_params_to_preset(preset: &mut Value, required_params: Value) {
    if let (Value::Object(preset_map), Value::Object(required_params_map)) =
        (preset, required_params)
    {
        for (key, value) in required_params_map {
            assert!(
                !preset_map.contains_key(&key),
                "Required parameter already exists in the preset: {:?}",
                key
            );
            preset_map.insert(key, value);
        }
    } else {
        panic!("Expecting JSON object dictionary objects");
    }
}
