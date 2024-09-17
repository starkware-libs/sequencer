use starknet_batcher::batcher::{create_batcher, Batcher};
use starknet_consensus_manager::consensus_manager::ConsensusManager;
use starknet_gateway::gateway::{create_gateway, Gateway};
use starknet_mempool::mempool::Mempool;

use crate::communication::MempoolNodeClients;
use crate::config::MempoolNodeConfig;

pub struct Components {
    pub batcher: Option<Batcher>,
    pub consensus_manager: Option<ConsensusManager>,
    pub gateway: Option<Gateway>,
    pub mempool: Option<Mempool>,
}

pub fn create_components(config: &MempoolNodeConfig, clients: &MempoolNodeClients) -> Components {
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

    let mempool = if config.components.mempool.execute { Some(Mempool::empty()) } else { None };

    Components { batcher, consensus_manager, gateway, mempool }
}
