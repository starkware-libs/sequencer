use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use serde_json::{Map, Value};
use tracing::{error, info};
use validator::ValidationError;

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
