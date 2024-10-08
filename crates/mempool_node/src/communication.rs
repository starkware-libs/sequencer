use std::sync::Arc;

use starknet_batcher_types::communication::{
    BatcherRequestAndResponseSender,
    LocalBatcherClient,
    RemoteBatcherClient,
    SharedBatcherClient,
};
use starknet_consensus_manager_types::communication::{
    ConsensusManagerRequestAndResponseSender,
    LocalConsensusManagerClient,
    RemoteConsensusManagerClient,
    SharedConsensusManagerClient,
};
use starknet_gateway_types::communication::{
    GatewayRequestAndResponseSender,
    LocalGatewayClient,
    RemoteGatewayClient,
    SharedGatewayClient,
};
use starknet_mempool_infra::component_definitions::ComponentCommunication;
use starknet_mempool_types::communication::{
    LocalMempoolClient,
    MempoolRequestAndResponseSender,
    RemoteMempoolClient,
    SharedMempoolClient,
};
use tokio::sync::mpsc::{channel, Receiver, Sender};

use crate::config::{ComponentExecutionMode, SequencerNodeConfig};

pub struct SequencerNodeCommunication {
    batcher_channel: ComponentCommunication<BatcherRequestAndResponseSender>,
    /// TODO(Tsabary): remove the redundant consensus_manager_channel.
    consensus_manager_channel: ComponentCommunication<ConsensusManagerRequestAndResponseSender>,
    mempool_channel: ComponentCommunication<MempoolRequestAndResponseSender>,
    gateway_channel: ComponentCommunication<GatewayRequestAndResponseSender>,
}

impl SequencerNodeCommunication {
    pub fn take_batcher_tx(&mut self) -> Sender<BatcherRequestAndResponseSender> {
        self.batcher_channel.take_tx()
    }

    pub fn take_batcher_rx(&mut self) -> Receiver<BatcherRequestAndResponseSender> {
        self.batcher_channel.take_rx()
    }

    pub fn take_consensus_manager_tx(
        &mut self,
    ) -> Sender<ConsensusManagerRequestAndResponseSender> {
        self.consensus_manager_channel.take_tx()
    }

    pub fn take_consensus_manager_rx(
        &mut self,
    ) -> Receiver<ConsensusManagerRequestAndResponseSender> {
        self.consensus_manager_channel.take_rx()
    }

    pub fn take_mempool_tx(&mut self) -> Sender<MempoolRequestAndResponseSender> {
        self.mempool_channel.take_tx()
    }

    pub fn take_mempool_rx(&mut self) -> Receiver<MempoolRequestAndResponseSender> {
        self.mempool_channel.take_rx()
    }

    pub fn take_gateway_tx(&mut self) -> Sender<GatewayRequestAndResponseSender> {
        self.gateway_channel.take_tx()
    }

    pub fn take_gateway_rx(&mut self) -> Receiver<GatewayRequestAndResponseSender> {
        self.gateway_channel.take_rx()
    }
}

pub fn create_node_channels() -> SequencerNodeCommunication {
    const DEFAULT_INVOCATIONS_QUEUE_SIZE: usize = 32;
    let (tx_mempool, rx_mempool) =
        channel::<MempoolRequestAndResponseSender>(DEFAULT_INVOCATIONS_QUEUE_SIZE);

    let (tx_batcher, rx_batcher) =
        channel::<BatcherRequestAndResponseSender>(DEFAULT_INVOCATIONS_QUEUE_SIZE);

    let (tx_consensus_manager, rx_consensus_manager) =
        channel::<ConsensusManagerRequestAndResponseSender>(DEFAULT_INVOCATIONS_QUEUE_SIZE);

    let (tx_gateway, rx_gateway) =
        channel::<GatewayRequestAndResponseSender>(DEFAULT_INVOCATIONS_QUEUE_SIZE);

    SequencerNodeCommunication {
        mempool_channel: ComponentCommunication::new(Some(tx_mempool), Some(rx_mempool)),
        consensus_manager_channel: ComponentCommunication::new(
            Some(tx_consensus_manager),
            Some(rx_consensus_manager),
        ),
        batcher_channel: ComponentCommunication::new(Some(tx_batcher), Some(rx_batcher)),
        gateway_channel: ComponentCommunication::new(Some(tx_gateway), Some(rx_gateway)),
    }
}

pub struct SequencerNodeClients {
    batcher_client: Option<SharedBatcherClient>,
    consensus_manager_client: Option<SharedConsensusManagerClient>,
    mempool_client: Option<SharedMempoolClient>,
    gateway_client: Option<SharedGatewayClient>,
    // TODO (Lev): Change to Option<Box<dyn MemPoolClient>>.
}

impl SequencerNodeClients {
    pub fn get_batcher_client(&self) -> Option<SharedBatcherClient> {
        self.batcher_client.clone()
    }

    pub fn get_consensus_manager_client(&self) -> Option<SharedConsensusManagerClient> {
        self.consensus_manager_client.clone()
    }

    pub fn get_mempool_client(&self) -> Option<SharedMempoolClient> {
        self.mempool_client.clone()
    }

    pub fn get_gateway_client(&self) -> Option<SharedGatewayClient> {
        self.gateway_client.clone()
    }
}

// TODO(Nadin): Create SequencerNodeLocalClients and SequencerNodeRemoteClients structs, and update
// the return value accordingly.
pub fn create_node_clients(
    config: &SequencerNodeConfig,
    channels: &mut SequencerNodeCommunication,
) -> (SequencerNodeClients, SequencerNodeClients) {
    let local_batcher_client: Option<SharedBatcherClient> = match config.components.batcher.execute
    {
        true => Some(Arc::new(LocalBatcherClient::new(channels.take_batcher_tx()))),
        false => None,
    };
    let local_consensus_manager_client: Option<SharedConsensusManagerClient> =
        match config.components.consensus_manager.execute {
            true => Some(Arc::new(LocalConsensusManagerClient::new(
                channels.take_consensus_manager_tx(),
            ))),
            false => None,
        };
    let local_mempool_client: Option<SharedMempoolClient> = match config.components.mempool.execute
    {
        true => Some(Arc::new(LocalMempoolClient::new(channels.take_mempool_tx()))),
        false => None,
    };
    let local_gateway_client: Option<SharedGatewayClient> = match config.components.gateway.execute
    {
        true => Some(Arc::new(LocalGatewayClient::new(channels.take_gateway_tx()))),
        false => None,
    };
    let remote_batcher_client: Option<SharedBatcherClient> =
        match (config.components.batcher.execute, config.components.batcher.execution_mode) {
            (true, ComponentExecutionMode::Remote) => Some(Arc::new(RemoteBatcherClient::new(
                config
                    .components
                    .batcher
                    .remote_config
                    .as_ref()
                    .expect("Remote config must be present when execution mode is Remote")
                    .client_config
                    .clone(),
            ))),
            _ => None,
        };
    let remote_consensus_manager_client: Option<SharedConsensusManagerClient> = match (
        config.components.consensus_manager.execute,
        config.components.consensus_manager.execution_mode,
    ) {
        (true, ComponentExecutionMode::Remote) => {
            Some(Arc::new(RemoteConsensusManagerClient::new(
                config
                    .components
                    .consensus_manager
                    .remote_config
                    .as_ref()
                    .expect("Remote config must be present when execution mode is Remote")
                    .client_config
                    .clone(),
            )))
        }
        _ => None,
    };
    let remote_mempool_client: Option<SharedMempoolClient> =
        match (config.components.mempool.execute, config.components.mempool.execution_mode) {
            (true, ComponentExecutionMode::Remote) => Some(Arc::new(RemoteMempoolClient::new(
                config
                    .components
                    .mempool
                    .remote_config
                    .as_ref()
                    .expect("Remote config must be present when execution mode is Remote")
                    .client_config
                    .clone(),
            ))),
            _ => None,
        };
    let remote_gateway_client: Option<SharedGatewayClient> =
        match (config.components.gateway.execute, config.components.gateway.execution_mode) {
            (true, ComponentExecutionMode::Remote) => Some(Arc::new(RemoteGatewayClient::new(
                config
                    .components
                    .gateway
                    .remote_config
                    .as_ref()
                    .expect("Remote config must be present when execution mode is Remote")
                    .client_config
                    .clone(),
            ))),
            _ => None,
        };

    let local_clients = SequencerNodeClients {
        batcher_client: local_batcher_client,
        consensus_manager_client: local_consensus_manager_client,
        mempool_client: local_mempool_client,
        gateway_client: local_gateway_client,
    };

    let remote_client = SequencerNodeClients {
        batcher_client: remote_batcher_client,
        consensus_manager_client: remote_consensus_manager_client,
        mempool_client: remote_mempool_client,
        gateway_client: remote_gateway_client,
    };

    (local_clients, remote_client)
}
