use std::sync::Arc;

use starknet_batcher_types::communication::{
    BatcherRequestAndResponseSender,
    LocalBatcherClient,
    SharedBatcherClient,
};
use starknet_gateway_types::communication::{
    GatewayRequestAndResponseSender,
    LocalGatewayClient,
    SharedGatewayClient,
};
use starknet_mempool_infra::component_definitions::ComponentCommunication;
use starknet_mempool_types::communication::{
    LocalMempoolClient,
    MempoolRequestAndResponseSender,
    SharedMempoolClient,
};
use tokio::sync::mpsc::{channel, Receiver, Sender};

use crate::config::{ComponentExecutionMode, SequencerNodeConfig};

pub struct SequencerNodeCommunication {
    batcher_channel: ComponentCommunication<BatcherRequestAndResponseSender>,
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

    let (tx_gateway, rx_gateway) =
        channel::<GatewayRequestAndResponseSender>(DEFAULT_INVOCATIONS_QUEUE_SIZE);

    SequencerNodeCommunication {
        mempool_channel: ComponentCommunication::new(Some(tx_mempool), Some(rx_mempool)),
        batcher_channel: ComponentCommunication::new(Some(tx_batcher), Some(rx_batcher)),
        gateway_channel: ComponentCommunication::new(Some(tx_gateway), Some(rx_gateway)),
    }
}

pub struct SequencerNodeClients {
    batcher_client: Option<SharedBatcherClient>,
    mempool_client: Option<SharedMempoolClient>,
    gateway_client: Option<SharedGatewayClient>,
    // TODO (Lev): Change to Option<Box<dyn MemPoolClient>>.
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
}

pub fn create_node_clients(
    config: &SequencerNodeConfig,
    channels: &mut SequencerNodeCommunication,
) -> SequencerNodeClients {
    let batcher_client: Option<SharedBatcherClient> = match config.components.batcher.execution_mode
    {
        ComponentExecutionMode::LocalExecution { enable_remote_connection: false } => {
            Some(Arc::new(LocalBatcherClient::new(channels.take_batcher_tx())))
        }
        ComponentExecutionMode::LocalExecution { enable_remote_connection: true } => None,
        ComponentExecutionMode::SkipComponent => None,
    };
    let mempool_client: Option<SharedMempoolClient> = match config.components.mempool.execution_mode
    {
        ComponentExecutionMode::LocalExecution { enable_remote_connection: false } => {
            Some(Arc::new(LocalMempoolClient::new(channels.take_mempool_tx())))
        }
        ComponentExecutionMode::LocalExecution { enable_remote_connection: true } => None,
        ComponentExecutionMode::SkipComponent => None,
    };
    let gateway_client: Option<SharedGatewayClient> = match config.components.gateway.execution_mode
    {
        ComponentExecutionMode::LocalExecution { enable_remote_connection: false } => {
            Some(Arc::new(LocalGatewayClient::new(channels.take_gateway_tx())))
        }
        ComponentExecutionMode::LocalExecution { enable_remote_connection: true } => None,
        ComponentExecutionMode::SkipComponent => None,
    };
    SequencerNodeClients { batcher_client, mempool_client, gateway_client }
}
