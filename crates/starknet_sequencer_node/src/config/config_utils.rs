use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use papyrus_config::dumping::{
    combine_config_map_and_pointers,
    ConfigPointers,
    Pointers,
    SerializeConfig,
};
use serde_json::{to_value, Map, Value};
use tracing::{error, info};
use validator::ValidationError;

use super::node_config::POINTER_TARGET_VALUE;
use crate::config::definitions::ConfigPointersMap;
use crate::config::node_config::{
    SequencerNodeConfig,
    CONFIG_NON_POINTERS_WHITELIST,
    CONFIG_POINTERS,
};

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

/// Dumps the input JSON data to a file at the specified path.
pub fn dump_json_data(json_data: Value, file_path: &PathBuf) {
    // Serialize the JSON data to a pretty-printed string
    let json_string = serde_json::to_string_pretty(&json_data).unwrap();

    // Write the JSON string to a file
    let mut file = File::create(file_path).unwrap();
    file.write_all(json_string.as_bytes()).unwrap();

    // Add an extra newline after the JSON content.
    file.write_all(b"\n").unwrap();

    file.flush().unwrap();

    info!("Writing required config changes to: {:?}", file_path);
}

pub fn dump_config_file(
    config: SequencerNodeConfig,
    pointers: &ConfigPointers,
    non_pointer_params: &Pointers,
    config_path: &PathBuf,
) {
    // Create the entire mapping of the config and the pointers, without the required params.
    let config_as_map =
        combine_config_map_and_pointers(config.dump(), pointers, non_pointer_params).unwrap();

    // Extract only the required fields from the config map.
    let preset = config_to_preset(&config_as_map);

    validate_all_pointer_targets_set(preset.clone()).expect("Pointer target not set");

    // Dump the preset to a file, return its path.
    dump_json_data(preset, config_path);
    assert!(config_path.exists(), "File does not exist: {:?}", config_path);
}

pub fn validate_all_pointer_targets_set(preset: Value) -> Result<(), ValidationError> {
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

pub fn create_app_config(config_path: PathBuf) {
    let config = SequencerNodeConfig::default();

    // Update config pointer values.
    let mut config_pointers_map = ConfigPointersMap::new(CONFIG_POINTERS.clone());
    config_pointers_map.change_target_value(
        "chain_id",
        to_value(config.l1_scraper_config.chain_id.clone()).expect("Failed to serialize ChainId"),
    );
    config_pointers_map.change_target_value(
        "eth_fee_token_address",
        to_value(
            config
                .batcher_config
                .block_builder_config
                .chain_info
                .fee_token_addresses
                .eth_fee_token_address,
        )
        .expect("Failed to serialize ContractAddress"),
    );
    config_pointers_map.change_target_value(
        "strk_fee_token_address",
        to_value(
            config
                .batcher_config
                .block_builder_config
                .chain_info
                .fee_token_addresses
                .strk_fee_token_address,
        )
        .expect("Failed to serialize ContractAddress"),
    );
    config_pointers_map.change_target_value(
        "validator_id",
        to_value(config.consensus_manager_config.consensus_config.validator_id)
            .expect("Failed to serialize ContractAddress"),
    );
    config_pointers_map.change_target_value(
        "recorder_url",
        to_value(config.consensus_manager_config.cende_config.recorder_url.clone())
            .expect("Failed to serialize Url"),
    );
    dump_config_file(
        config,
        &config_pointers_map.clone().into(),
        &CONFIG_NON_POINTERS_WHITELIST,
        &config_path,
    );
}
