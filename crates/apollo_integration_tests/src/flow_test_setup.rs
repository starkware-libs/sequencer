use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

<<<<<<< HEAD
use apollo_config::converters::UrlAndHeaders;
||||||| 01792faa8
use alloy::node_bindings::AnvilInstance;
use apollo_config::converters::UrlAndHeaders;
=======
use alloy::node_bindings::AnvilInstance;
>>>>>>> origin/main-v0.14.1
use apollo_consensus_manager::config::ConsensusManagerConfig;
use apollo_http_server::config::HttpServerConfig;
use apollo_http_server::test_utils::HttpTestClient;
use apollo_infra_utils::test_utils::AvailablePorts;
use apollo_l1_gas_price::eth_to_strk_oracle::EthToStrkOracleConfig;
use apollo_mempool_p2p::config::MempoolP2pConfig;
use apollo_monitoring_endpoint::config::MonitoringEndpointConfig;
use apollo_monitoring_endpoint::test_utils::MonitoringClient;
use apollo_network::gossipsub_impl::Topic;
use apollo_network::network_manager::test_utils::{
    create_connected_network_configs,
    network_config_into_broadcast_channels,
};
use apollo_network::network_manager::BroadcastTopicChannels;
use apollo_node::clients::SequencerNodeClients;
use apollo_node::config::component_config::ComponentConfig;
use apollo_node::config::node_config::SequencerNodeConfig;
use apollo_node::servers::run_component_servers;
use apollo_node::utils::create_node_modules;
use apollo_protobuf::consensus::{HeightAndRound, ProposalPart, StreamMessage, StreamMessageBody};
use apollo_state_sync::config::StateSyncConfig;
use apollo_storage::StorageConfig;
use blockifier::context::ChainInfo;
use futures::StreamExt;
use mempool_test_utils::starknet_api_test_utils::{
    AccountTransactionGenerator,
    MultiAccountTransactionGenerator,
};
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerConfig;
use papyrus_base_layer::test_utils::{
    make_block_history_on_anvil,
    ARBITRARY_ANVIL_L1_ACCOUNT_ADDRESS,
    OTHER_ARBITRARY_ANVIL_L1_ACCOUNT_ADDRESS,
};
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
use starknet_types_core::felt::Felt;
use tokio::sync::Mutex;
use tracing::{debug, instrument, Instrument};

use crate::anvil_base_layer::AnvilBaseLayer;
use crate::state_reader::{StorageTestHandles, StorageTestSetup};
use crate::utils::{
    create_consensus_manager_configs_from_network_configs,
    create_mempool_p2p_configs,
    create_node_config,
    create_state_sync_configs,
    set_validator_id,
    spawn_local_eth_to_strk_oracle,
    spawn_local_success_recorder,
    AccumulatedTransactions,
};

pub const NUM_OF_SEQUENCERS: usize = 2;
const SEQUENCER_0: usize = 0;
const SEQUENCER_1: usize = 1;
const BUILDER_BASE_ADDRESS: Felt = Felt::from_hex_unchecked("0x42");

// The number of fake transactions sent to L1 before test begins.
const NUM_L1_TRANSACTIONS: usize = 10;

pub struct FlowTestSetup {
    pub sequencer_0: FlowSequencerSetup,
    pub sequencer_1: FlowSequencerSetup,

    // Ethereum base layer coupled with an Anvil server instance, the server is dropped when the
    // instance is dropped.
    pub anvil_base_layer: AnvilBaseLayer,

    // The transactions that were streamed in the consensus proposals, used for asserting the right
    // transactions are batched.
    pub accumulated_txs: Arc<Mutex<AccumulatedTransactions>>,
}

impl FlowTestSetup {
    #[instrument(skip(tx_generator), level = "debug")]
    pub async fn new_from_tx_generator(
        tx_generator: &MultiAccountTransactionGenerator,
        test_unique_index: u16,
        block_max_capacity_sierra_gas: GasAmount,
        allow_bootstrap_txs: bool,
    ) -> Self {
        let chain_info = ChainInfo::create_for_testing();
        let mut available_ports = AvailablePorts::new(test_unique_index, 0);

        let accounts = tx_generator.accounts();
        let (consensus_manager_configs, consensus_proposals_channels) =
            create_consensus_manager_configs_and_channels(
                available_ports.get_next_ports(NUM_OF_SEQUENCERS + 1),
                &chain_info.chain_id,
            );
        let [sequencer_0_consensus_manager_config, sequencer_1_consensus_manager_config] =
            consensus_manager_configs.try_into().unwrap();

        let ports = available_ports.get_next_ports(NUM_OF_SEQUENCERS);
        let mempool_p2p_configs = create_mempool_p2p_configs(chain_info.chain_id.clone(), ports);
        let [sequencer_0_mempool_p2p_config, sequencer_1_mempool_p2p_config] =
            mempool_p2p_configs.try_into().unwrap();

        let [sequencer_0_state_sync_config, sequencer_1_state_sync_config] =
            create_state_sync_configs(
                StorageConfig::default(),
                available_ports.get_next_ports(NUM_OF_SEQUENCERS),
                available_ports.get_next_ports(NUM_OF_SEQUENCERS),
            )
            .try_into()
            .unwrap();

        let anvil_base_layer = AnvilBaseLayer::new().await;
        let base_layer_config = anvil_base_layer.ethereum_base_layer.config.clone();

        // Send some transactions to L1 so it has a history of blocks to scrape gas prices from.
        let sender_address = ARBITRARY_ANVIL_L1_ACCOUNT_ADDRESS;
        let receiver_address = OTHER_ARBITRARY_ANVIL_L1_ACCOUNT_ADDRESS;
        make_block_history_on_anvil(
            sender_address,
            receiver_address,
            base_layer_config.clone(),
            NUM_L1_TRANSACTIONS,
        )
        .await;

        // TODO(Itamar): Remove txs collector logic when flow tests are stable enough.
        // Spawn a thread that listens to proposals and collects batched transactions.
        let accumulated_txs = Arc::new(Mutex::new(AccumulatedTransactions::default()));
        let tx_collector_task = TxCollector {
            consensus_proposals_channels,
            accumulated_txs: accumulated_txs.clone(),
            chain_id: chain_info.chain_id.clone(),
        };

        tokio::spawn(tx_collector_task.collect_streamd_txs().in_current_span());

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
            allow_bootstrap_txs,
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
            allow_bootstrap_txs,
        )
        .await;

        Self { sequencer_0, sequencer_1, anvil_base_layer, accumulated_txs }
    }

    pub fn chain_id(&self) -> &ChainId {
        // TODO(Arni): Get the chain ID from a shared canonic location.
        &self
            .sequencer_0
            .node_config
            .batcher_config
            .as_ref()
            .unwrap()
            .block_builder_config
            .chain_info
            .chain_id
    }

    pub async fn send_messages_to_l2(&self, l1_handlers: &[L1HandlerTransaction]) {
        for l1_handler in l1_handlers {
            self.anvil_base_layer.send_message_to_l2(l1_handler).await;
        }
    }
}

pub struct FlowSequencerSetup {
    /// Used to differentiate between different sequencer nodes.
    pub node_index: usize,

    // Client for adding transactions to the sequencer node.
    pub add_tx_http_client: HttpTestClient,

    // Handles for the storage files, maintained so the files are not deleted.
    pub storage_handles: StorageTestHandles,

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
        allow_bootstrap_txs: bool,
    ) -> Self {
        let path = None;
        let StorageTestSetup { storage_config, storage_handles } =
            StorageTestSetup::new(accounts, &chain_info, path);

        let (recorder_url, _join_handle) =
            spawn_local_success_recorder(available_ports.get_next_port());
        consensus_manager_config.cende_config.recorder_url = recorder_url;

        let (eth_to_strk_oracle_url_headers, _join_handle) =
            spawn_local_eth_to_strk_oracle(available_ports.get_next_port());
        let eth_to_strk_oracle_config = EthToStrkOracleConfig {
            url_header_list: Some(vec![eth_to_strk_oracle_url_headers]),
            ..Default::default()
        };

        let validator_id = set_validator_id(&mut consensus_manager_config, node_index);

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
        let (mut node_config, _config_pointers_map) = create_node_config(
            &mut available_ports,
            chain_info,
            storage_config,
            state_sync_config,
            consensus_manager_config,
            eth_to_strk_oracle_config,
            mempool_p2p_config,
            monitoring_endpoint_config,
            component_config,
            base_layer_config,
            block_max_capacity_sierra_gas,
            validator_id,
            allow_bootstrap_txs,
        );
        let num_l1_txs = u64::try_from(NUM_L1_TRANSACTIONS).unwrap();
        node_config.l1_gas_price_scraper_config.as_mut().unwrap().number_of_blocks_for_mean =
            num_l1_txs;
        node_config.l1_gas_price_provider_config.as_mut().unwrap().number_of_blocks_for_mean =
            num_l1_txs;

        debug!("Sequencer config: {:#?}", node_config);
        let (clients, servers) = create_node_modules(&node_config).await;

        let MonitoringEndpointConfig { ip, port, .. } =
            node_config.monitoring_endpoint_config.as_ref().unwrap().to_owned();
        let monitoring_client = MonitoringClient::new(SocketAddr::from((ip, port)));

        let HttpServerConfig { ip, port } =
            node_config.http_server_config.as_ref().unwrap().to_owned();
        let add_tx_http_client = HttpTestClient::new(SocketAddr::from((ip, port)));

        // Run the sequencer node.
        tokio::spawn(run_component_servers(servers));

        Self {
            node_index,
            add_tx_http_client,
            storage_handles,
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
    }

    let broadcast_channels = network_config_into_broadcast_channels(
        channels_network_config,
        Topic::new(consensus_manager_configs[0].proposals_topic.clone()),
    );

    (consensus_manager_configs, broadcast_channels)
}

// Collects batched transactions.
struct TxCollector {
    pub consensus_proposals_channels:
        BroadcastTopicChannels<StreamMessage<ProposalPart, HeightAndRound>>,
    pub accumulated_txs: Arc<Mutex<AccumulatedTransactions>>,
    pub chain_id: ChainId,
}

impl TxCollector {
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

            if message.message == apollo_protobuf::consensus::StreamMessageBody::Fin {
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
            "Expected the first message in the stream to have id 0, got {incoming_message_id}"
        );
        let StreamMessageBody::Content(ProposalPart::Init(incoming_proposal_init)) = init_message
        else {
            panic!("Expected an init message. Got: {init_message:?}")
        };

        self.accumulated_txs
            .lock()
            .await
            .start_round(incoming_proposal_init.height, incoming_proposal_init.round);

        let mut got_proposal_fin = false;
        let mut got_channel_fin = false;
        for i in 1_u64..messages_cache.len().try_into().unwrap() {
            let StreamMessage { message, stream_id, message_id: _ } =
                messages_cache.remove(&i).expect("Stream should have all consecutive messages");
            assert_eq!(stream_id, first_stream_id, "Expected the same stream id for all messages");
            match message {
                StreamMessageBody::Content(ProposalPart::Init(init)) => {
                    panic!("Unexpected init: {init:?}")
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
                    let received_tx_hashes: Vec<_> = transactions
                        .transactions
                        .iter()
                        .map(|tx| match tx {
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
                        })
                        .collect();
                    self.accumulated_txs.lock().await.add_transactions(&received_tx_hashes);
                }
                StreamMessageBody::Content(ProposalPart::ExecutedTransactionCount(_)) => {
                    // TODO(Asmaa): Add validation for executed transaction count when implemented.
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
