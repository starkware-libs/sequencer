use std::net::SocketAddr;

use mempool_test_utils::starknet_api_test_utils::{AccountId, MultiAccountTransactionGenerator};
use starknet_api::block::BlockNumber;
use starknet_sequencer_infra::test_utils::AvailablePorts;
use starknet_sequencer_node::config::component_config::ComponentConfig;
use starknet_sequencer_node::config::component_execution_config::{
    ActiveComponentExecutionConfig,
    ReactiveComponentExecutionConfig,
};
use starknet_sequencer_node::test_utils::node_runner::get_node_executable_path;
use tracing::info;

use crate::sequencer_manager::{verify_results, SequencerManager};
use crate::test_identifiers::TestIdentifier;

/// The number of consolidated local sequencers that participate in the test.
const N_CONSOLIDATED_SEQUENCERS: usize = 3;
/// The number of distributed remote sequencers that participate in the test.
const N_DISTRIBUTED_SEQUENCERS: usize = 2;

pub async fn end_to_end_integration(tx_generator: MultiAccountTransactionGenerator) {
    const EXPECTED_BLOCK_NUMBER: BlockNumber = BlockNumber(15);
    const N_TXS: usize = 50;
    const SENDER_ACCOUNT: AccountId = 0;
    let sender_address = tx_generator.account_with_id(SENDER_ACCOUNT).sender_address();

    info!("Checking that the sequencer node executable is present.");
    get_node_executable_path();

    // TODO(Nadin): Assign a dedicated set of available ports to each sequencer.
    let mut available_ports =
        AvailablePorts::new(TestIdentifier::EndToEndIntegrationTest.into(), 0);

    // TODO(Nadin): replace Vec<ComponentConfig> with a struct - DistributedNodeConfigs.
    let component_configs: Vec<Vec<ComponentConfig>> =
        create_consolidated_sequencer_configs(N_CONSOLIDATED_SEQUENCERS)
            .into_iter()
            .chain(
                create_distributed_node_configs(&mut available_ports, N_DISTRIBUTED_SEQUENCERS)
                    .into_iter(),
            )
            .collect();

    info!("Running integration test setup.");
    // Creating the storage for the test.
    let integration_test_setup =
        SequencerManager::run(&tx_generator, available_ports, component_configs).await;

    // Wait for the nodes to start.
    integration_test_setup.await_alive(5000, 50).await;

    // TODO(AlonH): Consider checking all sequencer storage readers.
    let batcher_storage_reader = integration_test_setup.batcher_storage_reader();

    // Run the integration test simulator.
    integration_test_setup
        .run_integration_test_simulator(tx_generator, N_TXS, SENDER_ACCOUNT)
        .await;

    integration_test_setup
        .await_execution(EXPECTED_BLOCK_NUMBER, batcher_storage_reader.clone())
        .await;

    info!("Shutting down nodes.");
    integration_test_setup.shutdown_nodes();

    // Verify the results.
    verify_results(sender_address, batcher_storage_reader, N_TXS).await;
}

// TODO(Nadin/Tsabary): find a better name for this function.
fn get_http_container_config(
    gateway_socket: SocketAddr,
    mempool_socket: SocketAddr,
    mempool_p2p_socket: SocketAddr,
    state_sync_socket: SocketAddr,
) -> ComponentConfig {
    let mut config = ComponentConfig::disabled();
    config.http_server = ActiveComponentExecutionConfig::default();
    config.gateway = ReactiveComponentExecutionConfig::local_with_remote_enabled(gateway_socket);
    config.mempool = ReactiveComponentExecutionConfig::local_with_remote_enabled(mempool_socket);
    config.mempool_p2p =
        ReactiveComponentExecutionConfig::local_with_remote_enabled(mempool_p2p_socket);
    config.state_sync = ReactiveComponentExecutionConfig::remote(state_sync_socket);
    config.monitoring_endpoint = ActiveComponentExecutionConfig::default();
    config
}

fn get_non_http_container_config(
    gateway_socket: SocketAddr,
    mempool_socket: SocketAddr,
    mempool_p2p_socket: SocketAddr,
    state_sync_socket: SocketAddr,
) -> ComponentConfig {
    ComponentConfig {
        http_server: ActiveComponentExecutionConfig::disabled(),
        monitoring_endpoint: Default::default(),
        gateway: ReactiveComponentExecutionConfig::remote(gateway_socket),
        mempool: ReactiveComponentExecutionConfig::remote(mempool_socket),
        mempool_p2p: ReactiveComponentExecutionConfig::remote(mempool_p2p_socket),
        state_sync: ReactiveComponentExecutionConfig::local_with_remote_enabled(state_sync_socket),
        ..ComponentConfig::default()
    }
}

/// Generates configurations for a specified number of distributed sequencer nodes,
/// each consisting of an HTTP component configuration and a non-HTTP component configuration.
/// returns a vector of vectors, where each inner vector contains the two configurations.
fn create_distributed_node_configs(
    available_ports: &mut AvailablePorts,
    distributed_sequencers_num: usize,
) -> Vec<Vec<ComponentConfig>> {
    std::iter::repeat_with(|| {
        let gateway_socket = available_ports.get_next_local_host_socket();
        let mempool_socket = available_ports.get_next_local_host_socket();
        let mempool_p2p_socket = available_ports.get_next_local_host_socket();
        let state_sync_socket = available_ports.get_next_local_host_socket();

        vec![
            get_http_container_config(
                gateway_socket,
                mempool_socket,
                mempool_p2p_socket,
                state_sync_socket,
            ),
            get_non_http_container_config(
                gateway_socket,
                mempool_socket,
                mempool_p2p_socket,
                state_sync_socket,
            ),
        ]
    })
    .take(distributed_sequencers_num)
    .collect()
}

fn create_consolidated_sequencer_configs(
    num_of_consolidated_nodes: usize,
) -> Vec<Vec<ComponentConfig>> {
    std::iter::repeat_with(|| vec![ComponentConfig::default()])
        .take(num_of_consolidated_nodes)
        .collect()
}
