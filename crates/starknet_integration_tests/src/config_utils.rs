use std::fs::File;
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;

use papyrus_config::dumping::{combine_config_map_and_pointers, SerializeConfig};
use serde_json::{json, Map, Value};
use starknet_sequencer_infra::component_definitions::{
    LocalServerConfig,
    RemoteClientConfig,
    RemoteServerConfig,
};
use starknet_sequencer_infra::test_utils::get_available_socket;
use starknet_sequencer_node::config::component_config::ComponentConfig;
use starknet_sequencer_node::config::component_execution_config::{
    ActiveComponentExecutionConfig,
    ReactiveComponentExecutionConfig,
    ReactiveComponentExecutionMode,
};
use starknet_sequencer_node::config::node_config::{
    SequencerNodeConfig,
    CONFIG_NON_POINTERS_WHITELIST,
    CONFIG_POINTERS,
};
use starknet_sequencer_node::config::test_utils::RequiredParams;
use tracing::info;

// TODO(Tsabary): Move here all config-related functions from "integration_test_utils.rs".

const NODE_CONFIG_CHANGES_FILE_PATH: &str = "node_integration_test_config_changes.json";

/// A utility macro that takes a list of config fields and returns a json dictionary with "field
/// name : field value" entries, where prefixed "config." name is removed from the entry key.
///
/// # Example (not running, to avoid function visibility modifications):
///
/// use serde_json::json;
/// struct ConfigStruct {
///    field_1: u32,
///    field_2: String,
///    field_3: u32,
/// }
/// let config = ConfigStruct { field_1: 1, field_2: "2".to_string() , field_3: 3};
/// let json_data = config_fields_to_json!(config.field_1, config.field_2);
/// assert_eq!(json_data, json!({"field_1": 1, "field_2": "2"}));
macro_rules! config_fields_to_json {
    ( $( $expr:expr ),+ , ) => {
        json!({
            $(
                strip_config_prefix(stringify!($expr)): $expr
            ),+
        })
    };
}

/// Creates a config file for the sequencer node for an integration test.
pub(crate) fn dump_config_file_changes(
    config: &SequencerNodeConfig,
    required_params: RequiredParams,
    dir: PathBuf,
) -> PathBuf {
    // Dump config changes file for the sequencer node.
    // TODO(Tsabary): auto dump the entirety of RequiredParams fields.
    let required_params_json = config_fields_to_json!(
        required_params.chain_id,
        required_params.eth_fee_token_address,
        required_params.strk_fee_token_address,
        required_params.validator_id,
    );

    // Create the entire mapping of the config and the pointers, without the required params.
    let config_as_map = combine_config_map_and_pointers(
        config.dump(),
        &CONFIG_POINTERS,
        &CONFIG_NON_POINTERS_WHITELIST,
    )
    .unwrap();

    dump_json_data(required_params_json.clone(), "required_params_json", PathBuf::from("."));
    dump_json_data(config_as_map.clone(), "config_as_map", PathBuf::from("."));

    // Extract only the required fields from the config map.
    let mut preset = config_to_preset(&config_as_map);
    dump_json_data(preset.clone(), "preset_before", PathBuf::from("."));

    // Add the required params to the preset.
    add_required_params_to_preset(&mut preset, &required_params_json);
    dump_json_data(preset.clone(), "preset_after", PathBuf::from("."));

    // Dump the preset to a file, return its path.
    let node_config_path = dump_json_data(preset, NODE_CONFIG_CHANGES_FILE_PATH, dir);
    assert!(node_config_path.exists(), "File does not exist: {:?}", node_config_path);
    node_config_path
}

/// Dumps the input JSON data to a file at the specified path.
fn dump_json_data(json_data: Value, path: &str, dir: PathBuf) -> PathBuf {
    let temp_dir_path = dir.join(path);
    // Serialize the JSON data to a pretty-printed string
    let json_string = serde_json::to_string_pretty(&json_data).unwrap();

    // Write the JSON string to a file
    let mut file = File::create(&temp_dir_path).unwrap();
    file.write_all(json_string.as_bytes()).unwrap();

    info!("Writing required config changes to: {:?}", &temp_dir_path);
    temp_dir_path
}

/// Strips the "config." and "required_params." prefixes from the input string.
fn strip_config_prefix(input: &str) -> &str {
    input
        .strip_prefix("config.")
        .or_else(|| input.strip_prefix("required_params."))
        .unwrap_or(input)
}

// TODO(Nadin): Refactor the following functions to be static methods of
// ReactiveComponentExecutionConfig.
pub fn get_disabled_component_config() -> ReactiveComponentExecutionConfig {
    ReactiveComponentExecutionConfig {
        execution_mode: ReactiveComponentExecutionMode::Disabled,
        local_server_config: None,
        remote_client_config: None,
        remote_server_config: None,
    }
}

pub fn get_remote_component_config(socket: SocketAddr) -> ReactiveComponentExecutionConfig {
    ReactiveComponentExecutionConfig {
        execution_mode: ReactiveComponentExecutionMode::Remote,
        local_server_config: None,
        remote_client_config: Some(RemoteClientConfig { socket, ..RemoteClientConfig::default() }),
        remote_server_config: None,
    }
}

pub fn get_local_with_remote_enabled_component_config(
    socket: SocketAddr,
) -> ReactiveComponentExecutionConfig {
    ReactiveComponentExecutionConfig {
        execution_mode: ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled,
        local_server_config: Some(LocalServerConfig::default()),
        remote_client_config: None,
        remote_server_config: Some(RemoteServerConfig { socket }),
    }
}

pub async fn get_http_only_component_config(gateway_socket: SocketAddr) -> ComponentConfig {
    ComponentConfig {
        http_server: ActiveComponentExecutionConfig::default(),
        gateway: get_remote_component_config(gateway_socket),
        monitoring_endpoint: Default::default(),
        batcher: get_disabled_component_config(),
        consensus_manager: ActiveComponentExecutionConfig::disabled(),
        mempool: get_disabled_component_config(),
        mempool_p2p: get_disabled_component_config(),
        state_sync: get_disabled_component_config(),
        l1_provider: get_disabled_component_config(),
    }
}

pub async fn get_non_http_component_config(gateway_socket: SocketAddr) -> ComponentConfig {
    ComponentConfig {
        http_server: ActiveComponentExecutionConfig::disabled(),
        monitoring_endpoint: Default::default(),
        gateway: get_local_with_remote_enabled_component_config(gateway_socket),
        ..ComponentConfig::default()
    }
}

pub async fn get_remote_flow_test_config() -> Vec<ComponentConfig> {
    let gateway_socket = get_available_socket().await;
    vec![
        get_http_only_component_config(gateway_socket).await,
        get_non_http_component_config(gateway_socket).await,
    ]
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
/// This function panics if the provided `config_map` is not a JSON dictionary object, with a
/// descriptive error message: ```text
/// Config map is not a JSON object: <actual value>
/// ```
///
/// # Example
/// ```rust
/// use serde_json::{json, Value};
///
/// let input = json!({
///     "setting1": { "value": "preset1", "metadata": "info1" },
///     "setting2": { "value": "preset2", "metadata": "info2" },
///     "setting3": { "metadata": "info3" }
/// });
///
/// let result = config_to_preset(&input);
/// println!("{}", result);
/// ```
///
/// The output will be:
/// ```json
/// {
///     "setting1": "preset1",
///     "setting2": "preset2"
/// }
/// ```
fn config_to_preset(config_map: &Value) -> Value {
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

/// Merges required parameters into an existing preset JSON object.
///
/// # Parameters
/// - `preset`: A mutable reference to a `serde_json::Value` representing the preset. It must be a
///   JSON dictionary object where additional parameters will be added or updated.
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
/// This function panics if either `preset` or `required_params` is not a JSON dictionary object
/// with the error: ```text
/// Expecting JSON object dictionary objects
/// ```
///
/// # Example
/// ```rust
/// use serde_json::{json, Value};
///
/// let mut preset = json!({
///     "param1": "value1",
///     "param2": "value2"
/// });
///
/// let required_params = json!({
///     "param2": "updated_value2",
///     "param3": "value3"
/// });
///
/// add_required_params_to_preset(&mut preset, &required_params);
///
/// println!("{}", preset);
/// ```
///
/// The output will be:
/// ```json
/// {
///     "param1": "value1",
///     "param2": "updated_value2",
///     "param3": "value3"
/// }
/// ```
fn add_required_params_to_preset(preset: &mut Value, required_params: &Value) {
    if let (Value::Object(preset_map), Value::Object(required_params_map)) =
        (preset, required_params)
    {
        for (key, value) in required_params_map {
            preset_map.insert(key.clone(), value.clone());
        }
    } else {
        panic!("Expecting JSON object dictionary objects");
    }
}
