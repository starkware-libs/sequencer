use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use serde_json::{json, Value};
use starknet_sequencer_node::config::test_utils::RequiredParams;
use starknet_sequencer_node::config::SequencerNodeConfig;
use tempfile::TempDir;
use tracing::info;
// TODO(Tsabary): Move here all config-related functions from "integration_test_utils.rs".

const NODE_CONFIG_CHANGES_FILE_PATH: &str = "node_integration_test_config_changes.json";
const TX_GEN_CONFIG_CHANGES_FILE_PATH: &str = "tx_gen_integration_test_config_changes.json";

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

// TODO(Tsabary): fix comment after removing run_test_tx_generator.
/// Returns config files to be supplied for the sequencer node and the transaction generator. Then
///
/// Sequencer node:
/// cargo run --bin starknet_sequencer_node -- --config_file NODE_CONFIG_CHANGES_FILE_PATH
/// Transaction generator:
/// cargo run --bin run_test_tx_generator -- --config_file TX_GEN_CONFIG_CHANGES_FILE_PATH
pub fn dump_config_file_changes(
    config: &SequencerNodeConfig,
    required_params: RequiredParams,
    dir: &TempDir,
) -> (PathBuf, PathBuf) {
    // Dump config changes file for the sequencer node.
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

    // TODO(Tsabary): should be deprecated.
    //  Dump config changes file for the transaction generator.
    let json_data = config_fields_to_json!(
        required_params.chain_id,
        required_params.eth_fee_token_address,
        required_params.strk_fee_token_address,
        config.http_server_config.ip,
        config.http_server_config.port,
    );
    let tx_gen_config_path = dump_json_data(json_data, TX_GEN_CONFIG_CHANGES_FILE_PATH, dir);
    assert!(tx_gen_config_path.exists(), "File does not exist: {:?}", tx_gen_config_path);

    (node_config_path, tx_gen_config_path)
}

/// Dumps the input JSON data to a file at the specified path.
fn dump_json_data(json_data: Value, path: &str, dir: &TempDir) -> PathBuf {
    let temp_dir_path = dir.path().join(path);
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
