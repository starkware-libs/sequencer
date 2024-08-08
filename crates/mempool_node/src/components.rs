use starknet_gateway::gateway::{create_gateway, Gateway};
use starknet_mempool::mempool::Mempool;

use crate::communication::MempoolNodeClients;
use crate::config::MempoolNodeConfig;

pub struct Components {
    pub gateway: Option<Gateway>,
    pub mempool: Option<Mempool>,
}

pub fn create_components(config: &MempoolNodeConfig, clients: &MempoolNodeClients) -> Components {
    let gateway = if config.components.gateway.execute {
        let mempool_client =
            clients.get_mempool_client().expect("Mempool Client should be available");

        Some(create_gateway(
            config.gateway_config.clone(),
            config.rpc_state_reader_config.clone(),
            config.compiler_config,
            mempool_client,
        ))
    } else {
        None
    };

    let mempool = if config.components.mempool.execute { Some(Mempool::empty()) } else { None };

    Components { gateway, mempool }
}
