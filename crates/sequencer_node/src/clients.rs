use std::sync::Arc;

use starknet_batcher_types::communication::{LocalBatcherClient, SharedBatcherClient};
use starknet_gateway_types::communication::{LocalGatewayClient, SharedGatewayClient};
use starknet_mempool_p2p_types::communication::{
    LocalMempoolP2pPropagatorClient,
    SharedMempoolP2pPropagatorClient,
};
use starknet_mempool_types::communication::{LocalMempoolClient, SharedMempoolClient};

use crate::communication::SequencerNodeCommunication;
use crate::config::{ComponentExecutionMode, SequencerNodeConfig};

pub struct SequencerNodeClients {
    batcher_client: Option<SharedBatcherClient>,
    mempool_client: Option<SharedMempoolClient>,
    gateway_client: Option<SharedGatewayClient>,
    // TODO (Lev): Change to Option<Box<dyn MemPoolClient>>.
    mempool_p2p_propagator_client: Option<SharedMempoolP2pPropagatorClient>,
}

impl SequencerNodeClients {
    pub fn get_batcher_client(&self) -> Option<SharedBatcherClient> {
        self.batcher_client.clone()
    }

    pub fn get_mempool_client(&self) -> Option<SharedMempoolClient> {
        self.mempool_client.clone()
    }

    pub fn get_gateway_client(&self) -> Option<SharedGatewayClient> {
        self.gateway_client.clone()
    }

    pub fn get_mempool_p2p_propagator_client(&self) -> Option<SharedMempoolP2pPropagatorClient> {
        self.mempool_p2p_propagator_client.clone()
    }
}

pub fn create_node_clients(
    config: &SequencerNodeConfig,
    channels: &mut SequencerNodeCommunication,
) -> SequencerNodeClients {
    let batcher_client: Option<SharedBatcherClient> = match config.components.batcher.execution_mode
    {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            Some(Arc::new(LocalBatcherClient::new(channels.take_batcher_tx())))
        }
        ComponentExecutionMode::Disabled => None,
    };
    let mempool_client: Option<SharedMempoolClient> = match config.components.mempool.execution_mode
    {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            Some(Arc::new(LocalMempoolClient::new(channels.take_mempool_tx())))
        }
        ComponentExecutionMode::Disabled => None,
    };
    let gateway_client: Option<SharedGatewayClient> = match config.components.gateway.execution_mode
    {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => {
            Some(Arc::new(LocalGatewayClient::new(channels.take_gateway_tx())))
        }
        ComponentExecutionMode::Disabled => None,
    };

    let mempool_p2p_propagator_client: Option<SharedMempoolP2pPropagatorClient> =
        match config.components.mempool.execution_mode {
            ComponentExecutionMode::LocalExecutionWithRemoteDisabled
            | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => Some(Arc::new(
                LocalMempoolP2pPropagatorClient::new(channels.take_mempool_p2p_propagator_tx()),
            )),
            ComponentExecutionMode::Disabled => None,
        };
    SequencerNodeClients {
        batcher_client,
        mempool_client,
        gateway_client,
        mempool_p2p_propagator_client,
    }
}
