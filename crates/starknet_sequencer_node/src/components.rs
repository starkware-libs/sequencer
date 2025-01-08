use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerContract;
use starknet_batcher::batcher::{create_batcher, Batcher};
use starknet_consensus_manager::consensus_manager::ConsensusManager;
use starknet_gateway::gateway::{create_gateway, Gateway};
use starknet_http_server::http_server::{create_http_server, HttpServer};
use starknet_l1_provider::l1_scraper::L1Scraper;
use starknet_l1_provider::{create_l1_provider, event_identifiers_to_track, L1Provider};
use starknet_mempool::communication::{create_mempool, MempoolCommunicationWrapper};
use starknet_mempool_p2p::create_p2p_propagator_and_runner;
use starknet_mempool_p2p::propagator::MempoolP2pPropagator;
use starknet_mempool_p2p::runner::MempoolP2pRunner;
use starknet_monitoring_endpoint::monitoring_endpoint::{
    create_monitoring_endpoint,
    MonitoringEndpoint,
};
use starknet_state_sync::runner::StateSyncRunner;
use starknet_state_sync::{create_state_sync_and_runner, StateSync};

use crate::clients::SequencerNodeClients;
use crate::config::component_execution_config::{
    ActiveComponentExecutionMode,
    ReactiveComponentExecutionMode,
};
use crate::config::node_config::SequencerNodeConfig;
use crate::version::VERSION_FULL;

pub struct SequencerNodeComponents {
    pub batcher: Option<Batcher>,
    pub consensus_manager: Option<ConsensusManager>,
    pub gateway: Option<Gateway>,
    pub http_server: Option<HttpServer>,
    pub l1_scraper: Option<L1Scraper<EthereumBaseLayerContract>>,
    pub l1_provider: Option<L1Provider>,
    pub mempool: Option<MempoolCommunicationWrapper>,
    pub monitoring_endpoint: Option<MonitoringEndpoint>,
    pub mempool_p2p_propagator: Option<MempoolP2pPropagator>,
    pub mempool_p2p_runner: Option<MempoolP2pRunner>,
    pub state_sync: Option<StateSync>,
    pub state_sync_runner: Option<StateSyncRunner>,
}

pub fn create_node_components(
    config: &SequencerNodeConfig,
    clients: &SequencerNodeClients,
) -> SequencerNodeComponents {
    let batcher = match config.components.batcher.execution_mode {
        ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            let mempool_client =
                clients.get_mempool_shared_client().expect("Mempool Client should be available");
            let l1_provider_client = clients
                .get_l1_provider_shared_client()
                .expect("L1 Provider Client should be available");
            Some(create_batcher(config.batcher_config.clone(), mempool_client, l1_provider_client))
        }
        ReactiveComponentExecutionMode::Disabled | ReactiveComponentExecutionMode::Remote => None,
    };
    let consensus_manager = match config.components.consensus_manager.execution_mode {
        ActiveComponentExecutionMode::Enabled => {
            let batcher_client =
                clients.get_batcher_shared_client().expect("Batcher Client should be available");
            let state_sync_client = clients
                .get_state_sync_shared_client()
                .expect("State Sync Client should be available");
            Some(ConsensusManager::new(
                config.consensus_manager_config.clone(),
                batcher_client,
                state_sync_client,
            ))
        }
        ActiveComponentExecutionMode::Disabled => None,
    };
    let gateway = match config.components.gateway.execution_mode {
        ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            let mempool_client =
                clients.get_mempool_shared_client().expect("Mempool Client should be available");
            let state_sync_client = clients
                .get_state_sync_shared_client()
                .expect("State Sync Client should be available");

            Some(create_gateway(
                config.gateway_config.clone(),
                state_sync_client,
                config.compiler_config.clone(),
                mempool_client,
            ))
        }
        ReactiveComponentExecutionMode::Disabled | ReactiveComponentExecutionMode::Remote => None,
    };
    let http_server = match config.components.http_server.execution_mode {
        ActiveComponentExecutionMode::Enabled => {
            let gateway_client =
                clients.get_gateway_shared_client().expect("Gateway Client should be available");

            Some(create_http_server(config.http_server_config.clone(), gateway_client))
        }
        ActiveComponentExecutionMode::Disabled => None,
    };

    let (mempool_p2p_propagator, mempool_p2p_runner) = match config
        .components
        .mempool_p2p
        .execution_mode
    {
        ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            let gateway_client =
                clients.get_gateway_shared_client().expect("Gateway Client should be available");
            let (mempool_p2p_propagator, mempool_p2p_runner) =
                create_p2p_propagator_and_runner(config.mempool_p2p_config.clone(), gateway_client);
            (Some(mempool_p2p_propagator), Some(mempool_p2p_runner))
        }
        ReactiveComponentExecutionMode::Disabled | ReactiveComponentExecutionMode::Remote => {
            (None, None)
        }
    };

    let mempool = match config.components.mempool.execution_mode {
        ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            let mempool_p2p_propagator_client = clients
                .get_mempool_p2p_propagator_shared_client()
                .expect("Propagator Client should be available");
            let mempool = create_mempool(mempool_p2p_propagator_client);
            Some(mempool)
        }
        ReactiveComponentExecutionMode::Disabled | ReactiveComponentExecutionMode::Remote => None,
    };

    let monitoring_endpoint = match config.components.monitoring_endpoint.execution_mode {
        ActiveComponentExecutionMode::Enabled => Some(create_monitoring_endpoint(
            config.monitoring_endpoint_config.clone(),
            VERSION_FULL,
        )),
        ActiveComponentExecutionMode::Disabled => None,
    };

    let (state_sync, state_sync_runner) = match config.components.state_sync.execution_mode {
        ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            let (state_sync, state_sync_runner) =
                create_state_sync_and_runner(config.state_sync_config.clone());
            (Some(state_sync), Some(state_sync_runner))
        }
        ReactiveComponentExecutionMode::Disabled | ReactiveComponentExecutionMode::Remote => {
            (None, None)
        }
    };

    let l1_provider = match config.components.l1_provider.execution_mode {
        ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            Some(create_l1_provider(config.l1_provider_config.clone()))
        }
        ReactiveComponentExecutionMode::Disabled | ReactiveComponentExecutionMode::Remote => None,
    };

    let l1_scraper = match config.components.l1_scraper.execution_mode {
        ActiveComponentExecutionMode::Enabled => {
            let l1_provider_client = clients.get_l1_provider_shared_client().unwrap();
            let l1_scraper_config = config.l1_scraper_config.clone();
            let base_layer = EthereumBaseLayerContract::new(config.base_layer_config.clone());

            Some(L1Scraper::new(
                l1_scraper_config,
                l1_provider_client,
                base_layer,
                event_identifiers_to_track(),
            ))
        }
        ActiveComponentExecutionMode::Disabled => None,
    };

    SequencerNodeComponents {
        batcher,
        consensus_manager,
        gateway,
        http_server,
        l1_scraper,
        l1_provider,
        mempool,
        monitoring_endpoint,
        mempool_p2p_propagator,
        mempool_p2p_runner,
        state_sync,
        state_sync_runner,
    }
}
