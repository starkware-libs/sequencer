use std::fs::File;
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;

use serde_json::{json, Value};
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
use starknet_sequencer_node::config::node_config::SequencerNodeConfig;
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

/// Creates a config file for the sequencer node for the end to end integration test.
pub(crate) fn dump_config_file_changes(
    config: &SequencerNodeConfig,
    required_params: RequiredParams,
    dir: PathBuf,
) -> PathBuf {
    // Dump config changes file for the sequencer node.
    // TODO(Tsabary): auto dump the entirety of RequiredParams fields.
    let json_data = config_fields_to_json!(
        required_params.chain_id,
        required_params.eth_fee_token_address,
        required_params.strk_fee_token_address,
        config.rpc_state_reader_config.json_rpc_version,
        config.rpc_state_reader_config.url,
        config.batcher_config.storage.db_config.path_prefix,
        config.http_server_config.ip,
        config.http_server_config.port,
        config.consensus_manager_config.consensus_config.start_height,
    );
    let node_config_path = dump_json_data(json_data, NODE_CONFIG_CHANGES_FILE_PATH, dir);
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
