use starknet_batcher::batcher::{create_batcher, Batcher};
use starknet_consensus_manager::consensus_manager::ConsensusManager;
use starknet_gateway::gateway::{create_gateway, Gateway};
use starknet_http_server::http_server::{create_http_server, HttpServer};
use starknet_mempool::mempool::Mempool;

use crate::communication::SequencerNodeClients;
use crate::config::MempoolNodeConfig;

pub struct SequencerNodeComponents {
    pub batcher: Option<Batcher>,
    pub consensus_manager: Option<ConsensusManager>,
    pub gateway: Option<Gateway>,
    pub http_server: Option<HttpServer>,
    pub mempool: Option<Mempool>,
}

pub fn create_components(
    config: &MempoolNodeConfig,
    clients: &SequencerNodeClients,
) -> SequencerNodeComponents {
    let batcher = if config.components.batcher.execute {
        let mempool_client =
            clients.get_mempool_client().expect("Mempool Client should be available");
        Some(create_batcher(config.batcher_config.clone(), mempool_client))
    } else {
        None
    };

    let consensus_manager = if config.components.consensus_manager.execute {
        let batcher_client =
            clients.get_batcher_client().expect("Batcher Client should be available");
        Some(ConsensusManager::new(config.consensus_manager_config.clone(), batcher_client))
    } else {
        None
    };

    let gateway = if config.components.gateway.execute {
        let mempool_client =
            clients.get_mempool_client().expect("Mempool Client should be available");

        Some(create_gateway(
            config.gateway_config.clone(),
            config.rpc_state_reader_config.clone(),
            config.compiler_config.clone(),
            mempool_client,
        ))
    } else {
        None
    };

    let http_server = if config.components.http_server.execute {
        let gateway_client =
            clients.get_gateway_client().expect("Gateway Client should be available");

        Some(create_http_server(config.http_server_config.clone(), gateway_client))
    } else {
        None
    };

    let mempool = if config.components.mempool.execute { Some(Mempool::empty()) } else { None };

    SequencerNodeComponents { batcher, consensus_manager, gateway, http_server, mempool }
}
