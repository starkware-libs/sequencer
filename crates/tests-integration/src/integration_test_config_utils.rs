use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;

use serde_json::{json, Value};
use starknet_mempool_node::config::SequencerNodeConfig;
use tokio::io::Result;
use tracing::info;

// TODO(Tsabary): Move here all config-related functions from "integration_test_utils.rs".

const CONFIG_PARAMETERS_PATH: &str = "integration_test_config_changes.json";
const TX_GEN_CONFIG_PARAMETERS_PATH: &str = "tx_gen_integration_test_config_changes.json";

/// Takes a list of config fields and returns a json dictionary with "field name : field value"
/// entries. Note that the prefixed "config." name is removed from the entry key.
macro_rules! config_fields_to_json {
    ( $( $expr:expr ),+ ) => {
        json!({
            $(
                strip_config_prefix(stringify!($expr)): $expr
            ),+
        })
    };
}

// TODO(Tsabary): Consider wrapping dumped config files in a temp dir.

/// Returns config files to be supplied for the sequencer node and the transaction generator. Then
///
/// Sequencer node:
/// cargo run --bin starknet_mempool_node -- --config_file CONFIG_PARAMETERS_PATH
/// Transaction generator:
/// cargo run --bin run_test_tx_generator -- --config_file TX_GEN_CONFIG_PARAMETERS_PATH
pub fn create_config_files_for_node_and_tx_generator(
    config: SequencerNodeConfig,
) -> anyhow::Result<()> {
    // Create config file for the sequencer node.
    let json_data = config_fields_to_json!(
        config.rpc_state_reader_config.json_rpc_version,
        config.rpc_state_reader_config.url,
        config.batcher_config.storage.db_config.path_prefix,
        config
            .gateway_config
            .stateful_tx_validator_config
            .chain_info
            .fee_token_addresses
            .eth_fee_token_address,
        config
            .gateway_config
            .stateful_tx_validator_config
            .chain_info
            .fee_token_addresses
            .strk_fee_token_address
    );
    dump_json_data(json_data, CONFIG_PARAMETERS_PATH)?;

    // Create config file for the transaction generator.
    let json_data =
        config_fields_to_json!(config.http_server_config.ip, config.http_server_config.port);
    dump_json_data(json_data, TX_GEN_CONFIG_PARAMETERS_PATH)?;

    Ok(())
}

fn dump_json_data(json_data: Value, path: &str) -> Result<()> {
    // Serialize the JSON data to a pretty-printed string
    let json_string = serde_json::to_string_pretty(&json_data).unwrap();

    // Write the JSON string to a file
    let mut file = File::create(path)?;
    file.write_all(json_string.as_bytes())?;
    info!("Writing JSON data to: {:?}", path);

    Ok(())
}

fn strip_config_prefix(input: &str) -> &str {
    input.strip_prefix("config.").unwrap_or(input)
}
