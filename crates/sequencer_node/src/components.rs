use starknet_batcher::batcher::{create_batcher, Batcher};
use starknet_consensus_manager::consensus_manager::ConsensusManager;
use starknet_gateway::gateway::{create_gateway, Gateway};
use starknet_http_server::http_server::{create_http_server, HttpServer};
use starknet_mempool::communication::{create_mempool, MempoolCommunicationWrapper};
use starknet_mempool_p2p::create_p2p_propagator_and_runner;
use starknet_mempool_p2p::propagator::MempoolP2pPropagator;
use starknet_mempool_p2p::runner::MempoolP2pRunner;
use starknet_monitoring_endpoint::monitoring_endpoint::{
    create_monitoring_endpoint,
    MonitoringEndpoint,
};

use crate::communication::SequencerNodeClients;
use crate::config::{ComponentExecutionMode, SequencerNodeConfig};
use crate::version::VERSION_FULL;

pub struct SequencerNodeComponents {
    pub batcher: Option<Batcher>,
    pub consensus_manager: Option<ConsensusManager>,
    pub gateway: Option<Gateway>,
    pub http_server: Option<HttpServer>,
    pub mempool: Option<MempoolCommunicationWrapper>,
    pub monitoring_endpoint: Option<MonitoringEndpoint>,
    pub mempool_p2p_propagator: Option<MempoolP2pPropagator>,
    pub mempool_p2p_runner: Option<MempoolP2pRunner>,
}

pub fn create_node_components(
    config: &SequencerNodeConfig,
    clients: &SequencerNodeClients,
) -> SequencerNodeComponents {
    let batcher = match config.components.batcher.execution_mode {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            let mempool_client =
                clients.get_mempool_client().expect("Mempool Client should be available");
            Some(create_batcher(config.batcher_config.clone(), mempool_client))
        }
        ComponentExecutionMode::Disabled => None,
    };
    let consensus_manager = match config.components.consensus_manager.execution_mode {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            let batcher_client =
                clients.get_batcher_client().expect("Batcher Client should be available");
            Some(ConsensusManager::new(config.consensus_manager_config.clone(), batcher_client))
        }
        ComponentExecutionMode::Disabled => None,
    };
    let gateway = match config.components.gateway.execution_mode {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            let mempool_client =
                clients.get_mempool_client().expect("Mempool Client should be available");

            Some(create_gateway(
                config.gateway_config.clone(),
                config.rpc_state_reader_config.clone(),
                config.compiler_config.clone(),
                mempool_client,
            ))
        }
        ComponentExecutionMode::Disabled => None,
    };
    let http_server = match config.components.http_server.execution_mode {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            let gateway_client =
                clients.get_gateway_client().expect("Gateway Client should be available");

            Some(create_http_server(config.http_server_config.clone(), gateway_client))
        }
        ComponentExecutionMode::Disabled => None,
    };

    let (mempool_p2p_propagator, mempool_p2p_runner) =
        match config.components.mempool_p2p.execution_mode {
            ComponentExecutionMode::LocalExecutionWithRemoteDisabled
            | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
                let gateway_client =
                    clients.get_gateway_client().expect("Gateway Client should be available");
                let (mempool_p2p_propagator, mempool_p2p_runner) = create_p2p_propagator_and_runner(
                    config.mempool_p2p_config.clone(),
                    gateway_client,
                );
                (Some(mempool_p2p_propagator), Some(mempool_p2p_runner))
            }
            ComponentExecutionMode::Disabled => (None, None),
        };

    let mempool = match config.components.mempool.execution_mode {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            let mempool_p2p_propagator_client = clients
                .get_mempool_p2p_propagator_client()
                .expect("Propagator Client should be available");
            let mempool = create_mempool(mempool_p2p_propagator_client);
            Some(mempool)
        }
        ComponentExecutionMode::Disabled => None,
    };

    let monitoring_endpoint = match config.components.monitoring_endpoint.execution_mode {
        ComponentExecutionMode::LocalExecutionWithRemoteEnabled => Some(
            create_monitoring_endpoint(config.monitoring_endpoint_config.clone(), VERSION_FULL),
        ),
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled => None,
        ComponentExecutionMode::Disabled => None,
    };

    SequencerNodeComponents {
        batcher,
        consensus_manager,
        gateway,
        http_server,
        mempool,
        monitoring_endpoint,
        mempool_p2p_propagator,
        mempool_p2p_runner,
    }
}
