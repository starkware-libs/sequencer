use std::future::pending;

use anyhow::Ok;
use starknet_integration_tests::integration_test_config_utils::dump_config_file_changes;
use starknet_integration_tests::integration_test_utils::{
    create_integration_test_config,
    create_integration_test_tx_generator,
};
use starknet_integration_tests::state_reader::{spawn_test_rpc_state_reader, StorageTestSetup};
use starknet_sequencer_infra::trace_util::configure_tracing;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing();
    info!("Running integration test setup for the sequencer node.");

    // Creating the storage for the test.
    let storage_for_test = StorageTestSetup::new(create_integration_test_tx_generator().accounts());

    // Spawn a papyrus rpc server for a papyrus storage reader.
    let rpc_server_addr =
        spawn_test_rpc_state_reader(storage_for_test.rpc_storage_reader, storage_for_test.chain_id)
            .await;

    // Derive the configuration for the sequencer node.
    let (config, chain_id) =
        create_integration_test_config(rpc_server_addr, storage_for_test.batcher_storage_config)
            .await;

    // Note: the batcher storage file handle is passed as a reference to maintain its ownership in
    // this scope, such that the handle is not dropped and the storage is maintained.
    dump_config_file_changes(config, chain_id)?;

    // Keep the program running so the rpc state reader server, its storage, and the batcher
    // storage, are all maintained.
    let () = pending().await;
    Ok(())

    // TODO(Tsabary): Find a way to stop the program once the test is done.
}
