use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use rstest::{fixture, rstest};
use starknet_integration_tests::integration_test_config_utils::dump_config_file_changes;
use starknet_integration_tests::integration_test_utils::{
    create_config,
    create_integration_test_tx_generator,
};
use starknet_integration_tests::state_reader::{spawn_test_rpc_state_reader, StorageTestSetup};
use starknet_sequencer_infra::trace_util::configure_tracing;
use tempfile::tempdir;
use tracing::info;

#[fixture]
fn tx_generator() -> MultiAccountTransactionGenerator {
    create_integration_test_tx_generator()
}

#[rstest]
#[tokio::test]
async fn test_end_to_end_integration(tx_generator: MultiAccountTransactionGenerator) {
    configure_tracing();
    info!("Running integration test setup.");

    // Creating the storage for the test.
    let storage_for_test = StorageTestSetup::new(tx_generator.accounts());

    // Spawn a papyrus rpc server for a papyrus storage reader.
    let rpc_server_addr =
        spawn_test_rpc_state_reader(storage_for_test.rpc_storage_reader, storage_for_test.chain_id)
            .await;

    // Derive the configuration for the sequencer node.
    let (config, required_params) =
        create_config(rpc_server_addr, storage_for_test.batcher_storage_config).await;

    // Note: the batcher storage file handle is passed as a reference to maintain its ownership in
    // this scope, such that the handle is not dropped and the storage is maintained.
    let temp_dir = tempdir().unwrap();
    // TODO(Tsabary): pass path instead of temp dir.

    let (_node_config_path, _) = dump_config_file_changes(&config, required_params, &temp_dir);
}
