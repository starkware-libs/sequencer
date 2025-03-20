use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use alloy::node_bindings::AnvilInstance;
use blockifier::context::ChainInfo;
use futures::StreamExt;
use mempool_test_utils::starknet_api_test_utils::{
    AccountTransactionGenerator,
    MultiAccountTransactionGenerator,
};
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerConfig;
use papyrus_base_layer::test_utils::{
    ethereum_base_layer_config_for_anvil,
    spawn_anvil_and_deploy_starknet_l1_contract,
    StarknetL1Contract,
};
use papyrus_network::gossipsub_impl::Topic;
use papyrus_network::network_manager::test_utils::{
    create_connected_network_configs,
    network_config_into_broadcast_channels,
};
use papyrus_network::network_manager::BroadcastTopicChannels;
use papyrus_protobuf::consensus::{HeightAndRound, ProposalPart, StreamMessage, StreamMessageBody};
use papyrus_storage::StorageConfig;
use starknet_api::block::BlockNumber;
use starknet_api::consensus_transaction::ConsensusTransaction;
use starknet_api::core::{ChainId, ContractAddress};
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::RpcTransaction;
use starknet_api::transaction::{
    L1HandlerTransaction,
    TransactionHash,
    TransactionHasher,
    TransactionVersion,
};
use starknet_consensus_manager::config::ConsensusManagerConfig;
use starknet_gateway_types::errors::GatewaySpecError;
use starknet_http_server::config::HttpServerConfig;
use starknet_http_server::test_utils::HttpTestClient;
use starknet_infra_utils::test_utils::AvailablePorts;
use starknet_mempool_p2p::config::MempoolP2pConfig;
use starknet_monitoring_endpoint::config::MonitoringEndpointConfig;
use starknet_monitoring_endpoint::test_utils::MonitoringClient;
use starknet_sequencer_node::clients::SequencerNodeClients;
use starknet_sequencer_node::config::component_config::ComponentConfig;
use starknet_sequencer_node::config::node_config::SequencerNodeConfig;
use starknet_sequencer_node::servers::run_component_servers;
use starknet_sequencer_node::utils::create_node_modules;
use starknet_state_sync::config::StateSyncConfig;
use starknet_types_core::felt::Felt;
use tempfile::TempDir;
use tokio::sync::Mutex;
use tracing::{debug, instrument};
use url::Url;

use crate::executable_setup::NodeExecutionId;
use crate::state_reader::StorageTestSetup;
use crate::utils::{
    create_consensus_manager_configs_from_network_configs,
    create_mempool_p2p_configs,
    create_node_config,
    create_state_sync_configs,
    send_message_to_l2,
    spawn_local_success_recorder,
    AccumulatedTransactions,
};

const SEQUENCER_0: usize = 0;
const SEQUENCER_1: usize = 1;
const SEQUENCER_INDICES: [usize; 2] = [SEQUENCER_0, SEQUENCER_1];
const BUILDER_BASE_ADDRESS: Felt = Felt::from_hex_unchecked("0x42");

pub struct FlowTestSetup {
    pub sequencer_0: FlowSequencerSetup,
    pub sequencer_1: FlowSequencerSetup,

    // Handle for L1 server: the server is dropped when handle is dropped.
    #[allow(dead_code)]
    l1_handle: AnvilInstance,
    starknet_l1_contract: StarknetL1Contract,

    // Channels for consensus proposals, used for asserting the right transactions are proposed.
    pub consensus_proposals_channels:
        BroadcastTopicChannels<StreamMessage<ProposalPart, HeightAndRound>>,
}

impl FlowTestSetup {
    #[instrument(skip(tx_generator), level = "debug")]
    pub async fn new_from_tx_generator(
        tx_generator: &MultiAccountTransactionGenerator,
        test_unique_index: u16,
        block_max_capacity_sierra_gas: GasAmount,
    ) -> Self {
        let chain_info = ChainInfo::create_for_testing();
        let mut available_ports = AvailablePorts::new(test_unique_index, 0);

        let accounts = tx_generator.accounts();
        let (consensus_manager_configs, consensus_proposals_channels) =
            create_consensus_manager_configs_and_channels(
                available_ports.get_next_ports(SEQUENCER_INDICES.len() + 1),
                &chain_info.chain_id,
            );
        let [sequencer_0_consensus_manager_config, sequencer_1_consensus_manager_config]: [ConsensusManagerConfig;
            2] = consensus_manager_configs.try_into().unwrap();

        let ports = available_ports.get_next_ports(SEQUENCER_INDICES.len());
        let mempool_p2p_configs = create_mempool_p2p_configs(chain_info.chain_id.clone(), ports);
        let [sequencer_0_mempool_p2p_config, sequencer_1_mempool_p2p_config]: [MempoolP2pConfig;
            2] = mempool_p2p_configs.try_into().unwrap();

        let [sequencer_0_state_sync_config, sequencer_1_state_sync_config]: [StateSyncConfig; 2] =
            create_state_sync_configs(StorageConfig::default(), available_ports.get_next_ports(2))
                .try_into()
                .unwrap();

        let base_layer_config =
            ethereum_base_layer_config_for_anvil(Some(available_ports.get_next_port()));
        let (anvil, starknet_l1_contract) =
            spawn_anvil_and_deploy_starknet_l1_contract(&base_layer_config).await;

        // Create nodes one after the other in order to make sure the ports are not overlapping.
        let sequencer_0 = FlowSequencerSetup::new(
            accounts.to_vec(),
            SEQUENCER_0,
            chain_info.clone(),
            base_layer_config.clone(),
            sequencer_0_consensus_manager_config,
            sequencer_0_mempool_p2p_config,
            AvailablePorts::new(test_unique_index, 1),
            sequencer_0_state_sync_config,
            block_max_capacity_sierra_gas,
        )
        .await;

        let sequencer_1 = FlowSequencerSetup::new(
            accounts.to_vec(),
            SEQUENCER_1,
            chain_info,
            base_layer_config,
            sequencer_1_consensus_manager_config,
            sequencer_1_mempool_p2p_config,
            AvailablePorts::new(test_unique_index, 2),
            sequencer_1_state_sync_config,
            block_max_capacity_sierra_gas,
        )
        .await;

        Self {
            sequencer_0,
            sequencer_1,
            l1_handle: anvil,
            starknet_l1_contract,
            consensus_proposals_channels,
        }
    }

    pub async fn assert_add_tx_error(&self, tx: RpcTransaction) -> GatewaySpecError {
        self.sequencer_0.add_tx_http_client.assert_add_tx_error(tx).await
    }

    pub fn chain_id(&self) -> &ChainId {
        // TODO(Arni): Get the chain ID from a shared canonic location.
        &self.sequencer_0.node_config.batcher_config.block_builder_config.chain_info.chain_id
    }

    pub async fn send_messages_to_l2(&self, l1_handler_txs: &[L1HandlerTransaction]) {
        use reqwest::Client;
        use serde_json::json;
        for l1_handler in l1_handler_txs {
            let url = self.l1_handle.endpoint_url();
            // Create an HTTP client
            let client = Client::new();

            // JSON-RPC request payload
            let request_body = json!({
                "jsonrpc": "2.0",
                "method": "eth_pendingTransactions",
                "params": [],
                "id": 1
            });

            // Send POST request
            let response = client.post(url).json(&request_body).send().await.unwrap();

            // Parse response
            let response_json: serde_json::Value = response.json().await.unwrap();
            println!("Mempool Transactions: {:#?}", response_json);

            send_message_to_l2(l1_handler, &self.starknet_l1_contract).await;
        }
    }
}

pub struct FlowSequencerSetup {
    /// Used to differentiate between different sequencer nodes.
    pub node_index: usize,

    // Client for adding transactions to the sequencer node.
    pub add_tx_http_client: HttpTestClient,

    // Handlers for the storage files, maintained so the files are not deleted.
    pub batcher_storage_file_handle: Option<TempDir>,
    pub state_sync_storage_file_handle: Option<TempDir>,
    pub class_manager_storage_file_handles: Option<starknet_class_manager::test_utils::FileHandles>,

    // Node configuration.
    pub node_config: SequencerNodeConfig,

    // Monitoring client.
    pub monitoring_client: MonitoringClient,

    // Retain clients to avoid closing communication channels, which crashes the server and
    // subsequently the test. This occurs for components who are wrapped by servers, but no other
    // component has their client, usually due to these clients being added in a later date.
    clients: SequencerNodeClients,
}

impl FlowSequencerSetup {
    #[allow(clippy::too_many_arguments)]
    #[instrument(skip(accounts, chain_info, consensus_manager_config), level = "debug")]
    pub async fn new(
        accounts: Vec<AccountTransactionGenerator>,
        node_index: usize,
        chain_info: ChainInfo,
        base_layer_config: EthereumBaseLayerConfig,
        mut consensus_manager_config: ConsensusManagerConfig,
        mempool_p2p_config: MempoolP2pConfig,
        mut available_ports: AvailablePorts,
        state_sync_config: StateSyncConfig,
        block_max_capacity_sierra_gas: GasAmount,
    ) -> Self {
        let path = None;
        let StorageTestSetup {
            batcher_storage_config,
            batcher_storage_handle,
            state_sync_storage_config,
            state_sync_storage_handle,
            class_manager_storage_config,
            class_manager_storage_handles,
        } = StorageTestSetup::new(accounts, &chain_info, path);

        let (recorder_url, _join_handle) =
            spawn_local_success_recorder(available_ports.get_next_port());
        consensus_manager_config.cende_config.recorder_url = recorder_url;

        let component_config = ComponentConfig::default();

        // Explicitly avoid collecting metrics in the monitoring endpoint; metrics are collected
        // using a global recorder, which fails when being set multiple times in the same
        // process, as in this test.
        let monitoring_endpoint_config = MonitoringEndpointConfig {
            port: available_ports.get_next_port(),
            collect_metrics: false,
            ..Default::default()
        };

        // Derive the configuration for the sequencer node.
        let (node_config, _config_pointers_map) = create_node_config(
            &mut available_ports,
            NodeExecutionId::new(node_index, 0),
            chain_info,
            batcher_storage_config,
            state_sync_storage_config,
            class_manager_storage_config,
            state_sync_config,
            consensus_manager_config,
            mempool_p2p_config,
            monitoring_endpoint_config,
            component_config,
            base_layer_config,
            block_max_capacity_sierra_gas,
        );

        debug!("Sequencer config: {:#?}", node_config);
        let (clients, servers) = create_node_modules(&node_config).await;

        let MonitoringEndpointConfig { ip, port, .. } = node_config.monitoring_endpoint_config;
        let monitoring_client = MonitoringClient::new(SocketAddr::from((ip, port)));

        let HttpServerConfig { ip, port } = node_config.http_server_config;
        let add_tx_http_client = HttpTestClient::new(SocketAddr::from((ip, port)));

        // Run the sequencer node.
        tokio::spawn(run_component_servers(servers));

        Self {
            node_index,
            add_tx_http_client,
            batcher_storage_file_handle: batcher_storage_handle,
            state_sync_storage_file_handle: state_sync_storage_handle,
            class_manager_storage_file_handles: class_manager_storage_handles,
            node_config,
            monitoring_client,
            clients,
        }
    }

    pub async fn assert_add_tx_success(&self, tx: RpcTransaction) -> TransactionHash {
        self.add_tx_http_client.assert_add_tx_success(tx).await
    }

    pub async fn batcher_height(&self) -> BlockNumber {
        self.clients.get_batcher_shared_client().unwrap().get_height().await.unwrap().height
    }
}

pub fn create_consensus_manager_configs_and_channels(
    ports: Vec<u16>,
    chain_id: &ChainId,
) -> (
    Vec<ConsensusManagerConfig>,
    BroadcastTopicChannels<StreamMessage<ProposalPart, HeightAndRound>>,
) {
    let mut network_configs = create_connected_network_configs(ports);

    // TODO(Tsabary): Need to also add a channel for votes, in addition to the proposals channel.
    let channels_network_config = network_configs.pop().unwrap();

    let n_network_configs = network_configs.len();
    let mut consensus_manager_configs = create_consensus_manager_configs_from_network_configs(
        network_configs,
        n_network_configs,
        chain_id,
    );

    for (i, config) in consensus_manager_configs.iter_mut().enumerate() {
        config.context_config.builder_address =
            ContractAddress::try_from(BUILDER_BASE_ADDRESS + Felt::from(i)).unwrap();
        config.eth_to_strk_oracle_config.base_url =
            Url::parse("https://eth_to_strk_oracle_url").expect("Should be a valid URL");
    }

    let broadcast_channels = network_config_into_broadcast_channels(
        channels_network_config,
        Topic::new(consensus_manager_configs[0].proposals_topic.clone()),
    );

    (consensus_manager_configs, broadcast_channels)
}

// Collects batched transactions.
struct _TxCollector {
    pub consensus_proposals_channels:
        BroadcastTopicChannels<StreamMessage<ProposalPart, HeightAndRound>>,
    pub accumulated_txs: Arc<Mutex<AccumulatedTransactions>>,
    pub chain_id: ChainId,
}

impl _TxCollector {
    #[instrument(skip(self))]
    pub async fn collect_streamd_txs(mut self) {
        loop {
            self.listen_to_broadcasted_messages().await;
        }
    }

    async fn listen_to_broadcasted_messages(&mut self) {
        let broadcasted_messages_receiver =
            &mut self.consensus_proposals_channels.broadcasted_messages_receiver;
        // Collect messages in a map so that validations will use the ordering defined by
        // `message_id`, meaning we ignore network reordering, like the StreamHandler.
        let mut messages_cache = HashMap::new();
        let mut last_message_id = 0;

        while let Some((Ok(message), _)) = broadcasted_messages_receiver.next().await {
            messages_cache.insert(message.message_id, message.clone());

            if message.message == papyrus_protobuf::consensus::StreamMessageBody::Fin {
                last_message_id = message.message_id;
            }
            // Check that we got the Fin message and all previous messages.
            if last_message_id > 0
                && (0..=last_message_id).all(|id| messages_cache.contains_key(&id))
            {
                break;
            }
        }

        let StreamMessage {
            stream_id: first_stream_id,
            message: init_message,
            message_id: incoming_message_id,
        } = messages_cache.remove(&0).expect("Stream is missing its first message");

        assert_eq!(
            incoming_message_id, 0,
            "Expected the first message in the stream to have id 0, got {}",
            incoming_message_id
        );
        let StreamMessageBody::Content(ProposalPart::Init(incoming_proposal_init)) = init_message
        else {
            panic!("Expected an init message. Got: {:?}", init_message)
        };

        let mut received_tx_hashes = Vec::new();
        let mut got_proposal_fin = false;
        let mut got_channel_fin = false;
        for i in 1_u64..messages_cache.len().try_into().unwrap() {
            let StreamMessage { message, stream_id, message_id: _ } =
                messages_cache.remove(&i).expect("Stream should have all consecutive messages");
            assert_eq!(stream_id, first_stream_id, "Expected the same stream id for all messages");
            match message {
                StreamMessageBody::Content(ProposalPart::Init(init)) => {
                    panic!("Unexpected init: {:?}", init)
                }
                StreamMessageBody::Content(ProposalPart::Fin(..)) => {
                    got_proposal_fin = true;
                }
                StreamMessageBody::Content(ProposalPart::BlockInfo(_)) => {
                    // TODO(Asmaa): Add validation for block info.
                }
                StreamMessageBody::Content(ProposalPart::Transactions(transactions)) => {
                    // TODO(Arni): add calculate_transaction_hash to consensus transaction and use
                    // it here.
                    received_tx_hashes.extend(transactions.transactions.iter().map(|tx| {
                        match tx {
                            ConsensusTransaction::RpcTransaction(tx) => {
                                let starknet_api_tx =
                                    starknet_api::transaction::Transaction::from(tx.clone());
                                starknet_api_tx.calculate_transaction_hash(&self.chain_id).unwrap()
                            }
                            ConsensusTransaction::L1Handler(tx) => tx
                                .calculate_transaction_hash(
                                    &self.chain_id,
                                    &TransactionVersion::ZERO,
                                )
                                .unwrap(),
                        }
                    }));

                    self.accumulated_txs.lock().await.add_transactions(
                        incoming_proposal_init.height,
                        incoming_proposal_init.round,
                        &received_tx_hashes,
                    );
                }
                StreamMessageBody::Fin => {
                    got_channel_fin = true;
                }
            }
            if got_proposal_fin && got_channel_fin {
                break;
            }
        }
    }
}
