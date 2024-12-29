use std::net::SocketAddr;

use blockifier::context::ChainInfo;
use mempool_test_utils::starknet_api_test_utils::{
    AccountTransactionGenerator,
    MultiAccountTransactionGenerator,
};
use papyrus_network::network_manager::BroadcastTopicChannels;
use papyrus_protobuf::consensus::{ProposalPart, StreamMessage};
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_consensus_manager::config::ConsensusManagerConfig;
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_http_server::config::HttpServerConfig;
use starknet_http_server::test_utils::HttpTestClient;
use starknet_mempool_p2p::config::MempoolP2pConfig;
use starknet_monitoring_endpoint::config::MonitoringEndpointConfig;
use starknet_monitoring_endpoint::test_utils::MonitoringClient;
use starknet_sequencer_infra::test_utils::AvailablePorts;
use starknet_sequencer_node::config::component_config::ComponentConfig;
use starknet_sequencer_node::config::node_config::SequencerNodeConfig;
use starknet_sequencer_node::servers::run_component_servers;
use starknet_sequencer_node::utils::create_node_modules;
use tempfile::TempDir;
use tracing::{debug, instrument};

use crate::state_reader::StorageTestSetup;
use crate::utils::{
    create_chain_info,
    create_consensus_manager_configs_and_channels,
    create_mempool_p2p_configs,
    create_node_config,
    spawn_success_recorder,
};

const SEQUENCER_0: usize = 0;
const SEQUENCER_1: usize = 1;
const SEQUENCER_INDICES: [usize; 2] = [SEQUENCER_0, SEQUENCER_1];

pub struct FlowTestSetup {
    pub sequencer_0: FlowSequencerSetup,
    pub sequencer_1: FlowSequencerSetup,

    // Channels for consensus proposals, used for asserting the right transactions are proposed.
    pub consensus_proposals_channels: BroadcastTopicChannels<StreamMessage<ProposalPart>>,
}

impl FlowTestSetup {
    #[instrument(skip(tx_generator), level = "debug")]
    pub async fn new_from_tx_generator(
        tx_generator: &MultiAccountTransactionGenerator,
        test_unique_index: u16,
    ) -> Self {
        let chain_info = create_chain_info();
        let mut available_ports = AvailablePorts::new(test_unique_index, 0);

        let accounts = tx_generator.accounts();
        let (consensus_manager_configs, consensus_proposals_channels) =
            create_consensus_manager_configs_and_channels(
                SEQUENCER_INDICES.len(),
                &mut available_ports,
            );
        let [sequencer_0_consensus_manager_config, sequencer_1_consensus_manager_config]: [ConsensusManagerConfig;
            2] = consensus_manager_configs.try_into().unwrap();

        let ports = available_ports.get_next_ports(SEQUENCER_INDICES.len());
        let mempool_p2p_configs = create_mempool_p2p_configs(chain_info.chain_id.clone(), ports);
        let [sequencer_0_mempool_p2p_config, sequencer_1_mempool_p2p_config]: [MempoolP2pConfig;
            2] = mempool_p2p_configs.try_into().unwrap();

        // Create nodes one after the other in order to make sure the ports are not overlapping.
        let sequencer_0 = FlowSequencerSetup::new(
            accounts.to_vec(),
            SEQUENCER_0,
            chain_info.clone(),
            sequencer_0_consensus_manager_config,
            sequencer_0_mempool_p2p_config,
            AvailablePorts::new(test_unique_index, 1),
        )
        .await;

        let sequencer_1 = FlowSequencerSetup::new(
            accounts.to_vec(),
            SEQUENCER_1,
            chain_info,
            sequencer_1_consensus_manager_config,
            sequencer_1_mempool_p2p_config,
            AvailablePorts::new(test_unique_index, 2),
        )
        .await;

        Self { sequencer_0, sequencer_1, consensus_proposals_channels }
    }

    pub async fn assert_add_tx_error(&self, tx: RpcTransaction) -> GatewaySpecError {
        self.sequencer_0.add_tx_http_client.assert_add_tx_error(tx).await
    }
}

pub struct FlowSequencerSetup {
    /// Used to differentiate between different sequencer nodes.
    pub sequencer_index: usize,

    // Client for adding transactions to the sequencer node.
    pub add_tx_http_client: HttpTestClient,

    // Handlers for the storage files, maintained so the files are not deleted.
    pub batcher_storage_file_handle: TempDir,
    pub state_sync_storage_file_handle: TempDir,

    // Node configuration.
    pub node_config: SequencerNodeConfig,

    // Monitoring client.
    pub monitoring_client: MonitoringClient,
}

impl FlowSequencerSetup {
    #[instrument(skip(accounts, chain_info, consensus_manager_config), level = "debug")]
    pub async fn new(
        accounts: Vec<AccountTransactionGenerator>,
        sequencer_index: usize,
        chain_info: ChainInfo,
        mut consensus_manager_config: ConsensusManagerConfig,
        mempool_p2p_config: MempoolP2pConfig,
        mut available_ports: AvailablePorts,
    ) -> Self {
        let storage_for_test = StorageTestSetup::new(accounts, &chain_info);

        let recorder_url = spawn_success_recorder(available_ports.get_next_port());
        consensus_manager_config.cende_config.recorder_url = recorder_url;

        let component_config = ComponentConfig::default();

        // Derive the configuration for the sequencer node.
        let (node_config, _required_params) = create_node_config(
            &mut available_ports,
            sequencer_index,
            chain_info,
            storage_for_test.batcher_storage_config,
            storage_for_test.state_sync_storage_config,
            consensus_manager_config,
            mempool_p2p_config,
            component_config,
        )
        .await;

        debug!("Sequencer config: {:#?}", node_config);
        let (_clients, servers) = create_node_modules(&node_config);

        let MonitoringEndpointConfig { ip, port, .. } = node_config.monitoring_endpoint_config;
        let monitoring_client = MonitoringClient::new(SocketAddr::from((ip, port)));

        let HttpServerConfig { ip, port } = node_config.http_server_config;
        let add_tx_http_client = HttpTestClient::new(SocketAddr::from((ip, port)));

        // Run the sequencer node.
        tokio::spawn(run_component_servers(servers));

        Self {
            sequencer_index,
            add_tx_http_client,
            batcher_storage_file_handle: storage_for_test.batcher_storage_handle,
            state_sync_storage_file_handle: storage_for_test.state_sync_storage_handle,
            node_config,
            monitoring_client,
        }
    }

    pub async fn assert_add_tx_success(&self, tx: RpcTransaction) -> TransactionHash {
        self.add_tx_http_client.assert_add_tx_success(tx).await
    }
}
