use std::future::pending;

use anyhow::Ok;
use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::CairoVersion;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use starknet_mempool_infra::trace_util::configure_tracing;
use starknet_mempool_integration_tests::integration_test_config_utils::create_config_files_for_node_and_tx_generator;
use starknet_mempool_integration_tests::integration_test_utils::create_config;
use starknet_mempool_integration_tests::state_reader::{
    spawn_test_rpc_state_reader,
    StorageTestSetup,
};
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing();
    info!("Running integration test setup for the sequencer node.");

    // TODO(Tsabary): Code duplication with the end-to-end test. Refactor to avoid it.
    let mut tx_generator: MultiAccountTransactionGenerator =
        MultiAccountTransactionGenerator::new();

    for account in [
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1),
        FeatureContract::AccountWithoutValidations(CairoVersion::Cairo0),
    ] {
        tx_generator.register_account_for_flow_test(account);
    }

    // Spawn a papyrus rpc server for a papyrus storage reader.
    let accounts = tx_generator.accounts();
    let storage_for_test = StorageTestSetup::new(accounts);

    // Spawn a papyrus rpc server for a papyrus storage reader.
    let rpc_server_addr = spawn_test_rpc_state_reader(storage_for_test.rpc_storage_reader).await;

    // Derive the configuration for the mempool node.
    let config = create_config(rpc_server_addr, storage_for_test.batcher_storage_config).await;

    // Note: the batcher storage file handle is passed as a reference to maintain its ownership in
    // this scope, such that the handle is not dropped and the storage is maintained.
    create_config_files_for_node_and_tx_generator(config)?;

    // Keep the program running so the rpc state reader server, its storage, and the batcher
    // storage, are maintained.
    let () = pending().await;
    Ok(())

    // TODO(Tsabary): Find a way to stop the program once the test is done.
}
