use std::fs::File;
use std::io::Write;

use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::CairoVersion;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use serde_json::json;
use starknet_mempool_infra::trace_util::configure_tracing;
use starknet_mempool_integration_tests::integration_test_utils::{
    create_config,
    test_rpc_state_reader_config,
};
use starknet_mempool_integration_tests::state_reader::spawn_test_rpc_state_reader;
use tokio::time::{sleep, Duration};
use tracing::info;

const CONFIG_PARAMETERS_PATH: &str = "integration_test_config_changes.json";
const TX_GEN_CONFIG_PARAMETERS_PATH: &str = "tx_gen_integration_test_config_changes.json";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    info!("Running integration test SETUP for the sequencer node.");

    let mut tx_generator: MultiAccountTransactionGenerator =
        MultiAccountTransactionGenerator::new();

    for account in [
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1),
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo0),
    ] {
        tx_generator.register_account_for_flow_test(account);
    }

    // Configure and start tracing.
    configure_tracing();

    // Spawn a papyrus rpc server for a papyrus storage reader.
    let (rpc_server_addr, gateway_storage_file_handle) =
        spawn_test_rpc_state_reader(tx_generator.accounts()).await;

    // Derive the configuration for the mempool node.
    let (_config, batcher_storage_file_handle) =
        create_config(rpc_server_addr, &gateway_storage_file_handle).await;
    let rpc_state_reader_config = test_rpc_state_reader_config(rpc_server_addr);

    // Create JSON data using the json! macro
    let json_data = json!({
        "rpc_state_reader_config.json_rpc_version": rpc_state_reader_config.json_rpc_version,
        "rpc_state_reader_config.url": rpc_state_reader_config.url,
        "batcher_config.storage.db_config.path_prefix": batcher_storage_file_handle.path().to_str().unwrap(),
        "chain_id": "",
        "components.consensus_manager.execute" : false,
    });

    // TODO: delete: "components.consensus_manager.execute" : false,

    // Serialize the JSON data to a pretty-printed string
    let json_string = serde_json::to_string_pretty(&json_data).unwrap();

    // Write the JSON string to a file
    let mut file = File::create(CONFIG_PARAMETERS_PATH)?;
    file.write_all(json_string.as_bytes())?;
    info!("Writing config changes: {:?}", CONFIG_PARAMETERS_PATH);


    // Create JSON data using the json! macro
    let json_data = json!({
        "http_server_config.ip": "0.0.0.0",
        "http_server_config.port": 8080,

    });


    // Serialize the JSON data to a pretty-printed string
    let json_string = serde_json::to_string_pretty(&json_data).unwrap();

    // Write the JSON string to a file
    let mut file = File::create(TX_GEN_CONFIG_PARAMETERS_PATH)?;
    file.write_all(json_string.as_bytes())?;
    info!("Writing config changes: {:?}", TX_GEN_CONFIG_PARAMETERS_PATH);



    // Need to use batcher_storage_file_handle so it's not dropped, e.g., in the following info
    // message.
    info!("Initializing batcher storage: {:?}", batcher_storage_file_handle.path());

    // Keep the program running so the rpc state reader task is maintained.
    loop {
        sleep(Duration::from_secs(1)).await;
    }
}
