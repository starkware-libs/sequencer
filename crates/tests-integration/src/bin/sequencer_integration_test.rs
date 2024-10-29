use anyhow::Ok;
use starknet_integration_tests::integration_test_config_utils::dump_config_file_changes;
use starknet_integration_tests::integration_test_utils::{
    create_integration_test_config,
    create_integration_test_tx_generator,
};
use starknet_integration_tests::state_reader::{spawn_test_rpc_state_reader, StorageTestSetup};
use starknet_sequencer_infra::trace_util::configure_tracing;
use starknet_sequencer_node::compilation::compile_node_with_status;
use tracing::{error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing();
    info!("Running integration test for the sequencer node.");

    // Compile the node
    info!("Compiling sequencer node.");
    if !compile_node_with_status() {
        error!("Failed to compile the node");
    };

    info!("Creating Papyrus storage for test.");
    let storage_for_test = StorageTestSetup::new(create_integration_test_tx_generator().accounts());

    info!("Spawning Papyrus RPC state reader for the Gateway.");
    let rpc_server_addr =
        spawn_test_rpc_state_reader(storage_for_test.rpc_storage_reader, storage_for_test.chain_id)
            .await;

    info!("Deriving Sequencer node configuration.");
    let (config, required_params) =
        create_integration_test_config(rpc_server_addr, storage_for_test.batcher_storage_config)
            .await;
    dump_config_file_changes(config, required_params)?;

    info!("Integration test completed successfully <3.");
    Ok(())
}
