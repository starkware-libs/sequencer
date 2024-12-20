use std::net::SocketAddr;
use std::path::PathBuf;

use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use papyrus_storage::StorageConfig;
use starknet_http_server::config::HttpServerConfig;
use starknet_http_server::test_utils::HttpTestClient;
use starknet_monitoring_endpoint::config::MonitoringEndpointConfig;
use starknet_monitoring_endpoint::test_utils::IsAliveClient;
use starknet_sequencer_infra::test_utils::AvailablePorts;
use tempfile::{tempdir, TempDir};

use crate::config_utils::dump_config_file_changes;
use crate::state_reader::{spawn_test_rpc_state_reader_with_socket, StorageTestSetup};
use crate::utils::{
    create_chain_info,
    create_config,
    create_consensus_manager_configs_and_channels,
    create_mempool_p2p_configs,
};

const SEQUENCER_INDEX: usize = 0;
const SEQUENCER_INDICES: [usize; 1] = [SEQUENCER_INDEX];

pub struct IntegrationTestSetup {
    // Client for adding transactions to the sequencer node.
    pub add_tx_http_client: HttpTestClient,
    // Client for checking liveness of the sequencer node.
    pub is_alive_test_client: IsAliveClient,
    // Path to the node configuration file.
    pub node_config_path: PathBuf,
    // Storage reader for the batcher.
    pub batcher_storage_config: StorageConfig,
    // Storage reader for the state sync.
    pub state_sync_storage_config: StorageConfig,
    // Available ports for the test.
    pub available_ports: AvailablePorts,
    // Handlers for the storage and config files, maintained so the files are not deleted. Since
    // these are only maintained to avoid dropping the handlers, private visibility suffices, and
    // as such, the '#[allow(dead_code)]' attributes are used to suppress the warning.
    #[allow(dead_code)]
    batcher_storage_handle: TempDir,
    #[allow(dead_code)]
    rpc_storage_handle: TempDir,
    #[allow(dead_code)]
    node_config_dir_handle: TempDir,
    #[allow(dead_code)]
    state_sync_storage_handle: TempDir,
}

impl IntegrationTestSetup {
    pub async fn new_from_tx_generator(
        tx_generator: &MultiAccountTransactionGenerator,
        test_unique_index: u16,
    ) -> Self {
        let mut available_ports = AvailablePorts::new(test_unique_index, 0);

        let chain_info = create_chain_info();
        // Creating the storage for the test.
        let storage_for_test = StorageTestSetup::new(tx_generator.accounts(), &chain_info);

        // Spawn a papyrus rpc server for a papyrus storage reader.
        let rpc_server_addr = spawn_test_rpc_state_reader_with_socket(
            storage_for_test.rpc_storage_reader,
            chain_info.chain_id.clone(),
            available_ports.get_next_local_host_socket(),
        )
        .await;

        let (mut consensus_manager_configs, _consensus_proposals_channels) =
            create_consensus_manager_configs_and_channels(
                SEQUENCER_INDICES.len(),
                &mut available_ports,
            );
        let mut mempool_p2p_configs = create_mempool_p2p_configs(
            SEQUENCER_INDICES.len(),
            chain_info.chain_id.clone(),
            &mut available_ports,
        );

        // Derive the configuration for the sequencer node.
        let (config, required_params) = create_config(
            &mut available_ports,
            SEQUENCER_INDEX,
            chain_info,
            rpc_server_addr,
            storage_for_test.batcher_storage_config,
            storage_for_test.state_sync_storage_config,
            consensus_manager_configs.pop().unwrap(),
            mempool_p2p_configs.pop().unwrap(),
        )
        .await;

        let node_config_dir_handle = tempdir().unwrap();
        let node_config_path = dump_config_file_changes(
            &config,
            required_params,
            node_config_dir_handle.path().to_path_buf(),
        );

        // Wait for the node to start.
        let MonitoringEndpointConfig { ip, port, .. } = config.monitoring_endpoint_config;
        let is_alive_test_client = IsAliveClient::new(SocketAddr::from((ip, port)));

        let HttpServerConfig { ip, port } = config.http_server_config;
        let add_tx_http_client = HttpTestClient::new(SocketAddr::from((ip, port)));

        IntegrationTestSetup {
            add_tx_http_client,
            is_alive_test_client,
            batcher_storage_handle: storage_for_test.batcher_storage_handle,
            batcher_storage_config: config.batcher_config.storage,
            rpc_storage_handle: storage_for_test.rpc_storage_handle,
            available_ports,
            node_config_dir_handle,
            node_config_path,
            state_sync_storage_handle: storage_for_test.state_sync_storage_handle,
            state_sync_storage_config: config.state_sync_config.storage_config,
        }
    }
}
