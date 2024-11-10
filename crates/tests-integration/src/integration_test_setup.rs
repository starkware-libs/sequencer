use std::net::SocketAddr;
use std::path::PathBuf;

use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use starknet_http_server::config::HttpServerConfig;
use starknet_monitoring_endpoint::config::MonitoringEndpointConfig;
use starknet_monitoring_endpoint::test_utils::IsAliveClient;
use tempfile::{tempdir, TempDir};

use crate::config_utils::dump_config_file_changes;
use crate::state_reader::{spawn_test_rpc_state_reader, StorageTestSetup};
use crate::utils::{create_config, HttpTestClient};

pub struct IntegrationTestSetup {
    // Client for adding transactions to the sequencer node.
    pub add_tx_http_client: HttpTestClient,

    // Client for checking liveness of the sequencer node.
    pub is_alive_test_client: IsAliveClient,

    // Path to the node configuration file.
    pub node_config_path: PathBuf,

    // Handlers for the storage and config files, maintained so the files are not deleted. Since
    // these are only maintained to avoid dropping the handlers, private visibility suffices, and
    // as such, the '#[allow(dead_code)]' attributes are used to suppress the warning.
    #[allow(dead_code)]
    batcher_storage_handle: TempDir,
    #[allow(dead_code)]
    rpc_storage_handle: TempDir,
    #[allow(dead_code)]
    node_config_dir_handle: TempDir,
}

impl IntegrationTestSetup {
    pub async fn new_from_tx_generator(tx_generator: &MultiAccountTransactionGenerator) -> Self {
        // Creating the storage for the test.
        let storage_for_test = StorageTestSetup::new(tx_generator.accounts());

        // Spawn a papyrus rpc server for a papyrus storage reader.
        let rpc_server_addr = spawn_test_rpc_state_reader(
            storage_for_test.rpc_storage_reader,
            storage_for_test.chain_id,
        )
        .await;

        // Derive the configuration for the sequencer node.
        let (config, required_params) =
            create_config(rpc_server_addr, storage_for_test.batcher_storage_config).await;

        // Note: the batcher storage file handle is passed as a reference to maintain its ownership
        // in this scope, such that the handle is not dropped and the storage is maintained.
        let node_config_dir_handle = tempdir().unwrap();
        // TODO(Tsabary): pass path instead of temp dir.
        let (node_config_path, _) =
            dump_config_file_changes(&config, required_params, &node_config_dir_handle);

        // Wait for the node to start.
        let MonitoringEndpointConfig { ip, port } = config.monitoring_endpoint_config;
        let is_alive_test_client = IsAliveClient::new(SocketAddr::from((ip, port)));

        let HttpServerConfig { ip, port } = config.http_server_config;
        let add_tx_http_client = HttpTestClient::new(SocketAddr::from((ip, port)));

        IntegrationTestSetup {
            add_tx_http_client,
            is_alive_test_client,
            batcher_storage_handle: storage_for_test.batcher_storage_handle,
            rpc_storage_handle: storage_for_test.rpc_storage_handle,
            node_config_dir_handle,
            node_config_path,
        }
    }
}
