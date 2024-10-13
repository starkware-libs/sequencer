use std::future::pending;

use anyhow::Ok;
use papyrus_config::dumping::SerializeConfig;
use starknet_mempool_infra::trace_util::configure_tracing;
use starknet_mempool_integration_tests::integration_test_config_utils::dump_config_file_changes;
use starknet_mempool_integration_tests::integration_test_utils::{
    create_config,
    create_integration_test_tx_generator,
};
use starknet_mempool_integration_tests::state_reader::{
    spawn_test_rpc_state_reader,
    StorageTestSetup,
};
use starknet_mempool_node::config::CONFIG_POINTERS;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing();
    info!("Running integration test setup for the sequencer node.");

    // Creating the storage for the test.
    let storage_for_test = StorageTestSetup::new(create_integration_test_tx_generator().accounts());

    let chain_id = storage_for_test.batcher_storage_config.db_config.chain_id.clone();

    info!("chain_id: {:?}", chain_id);

    info!("rpc_storage_handle path: {:?}", storage_for_test.rpc_storage_handle.path());
    info!("batcher_storage_handle path: {:?}", storage_for_test.batcher_storage_handle.path());

    // Spawn a papyrus rpc server for a papyrus storage reader.
    let rpc_server_addr = spawn_test_rpc_state_reader(storage_for_test.rpc_storage_reader,  storage_for_test.chain_id).await;

    // Derive the configuration for the sequencer node.
    let config = create_config(rpc_server_addr, storage_for_test.batcher_storage_config).await;



    info!("config chain id: {:?}", config.chain_id);
    info!("batcher chain id: {:?}", config.batcher_config.storage.db_config.chain_id);
    info!("gateway chain id: {:?}", config.gateway_config.chain_info.chain_id);

    info!("dumping config");
    config.dump_to_file(&CONFIG_POINTERS, "dump_config.json").expect("dump to file error");

    // Note: the batcher storage file handle is passed as a reference to maintain its ownership in
    // this scope, such that the handle is not dropped and the storage is maintained.
    dump_config_file_changes(config)?;



    // Keep the program running so the rpc state reader server, its storage, and the batcher
    // storage, are all maintained.
    let () = pending().await;
    Ok(())

    // TODO(Tsabary): Find a way to stop the program once the test is done.
}
