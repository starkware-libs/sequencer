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
use starknet_mempool_p2p_types::communication::{
    LocalMempoolP2pPropagatorClient,
    MempoolP2pPropagatorRequestAndResponseSender,
    SharedMempoolP2pPropagatorClient,
};
use starknet_mempool_types::communication::{
    LocalMempoolClient,
    MempoolRequestAndResponseSender,
    SharedMempoolClient,
};
use starknet_sequencer_infra::component_definitions::ComponentCommunication;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use crate::config::{ComponentExecutionMode, SequencerNodeConfig};

pub struct SequencerNodeCommunication {
    batcher_channel: ComponentCommunication<BatcherRequestAndResponseSender>,
    mempool_channel: ComponentCommunication<MempoolRequestAndResponseSender>,
    gateway_channel: ComponentCommunication<GatewayRequestAndResponseSender>,
    mempool_p2p_propagator_channel:
        ComponentCommunication<MempoolP2pPropagatorRequestAndResponseSender>,
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

    pub fn take_mempool_p2p_propagator_tx(
        &mut self,
    ) -> Sender<MempoolP2pPropagatorRequestAndResponseSender> {
        self.mempool_p2p_propagator_channel.take_tx()
    }
    pub fn take_mempool_p2p_propagator_rx(
        &mut self,
    ) -> Receiver<MempoolP2pPropagatorRequestAndResponseSender> {
        self.mempool_p2p_propagator_channel.take_rx()
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

    let (tx_mempool_p2p_propagator, rx_mempool_p2p_propagator) =
        channel::<MempoolP2pPropagatorRequestAndResponseSender>(DEFAULT_INVOCATIONS_QUEUE_SIZE);

    SequencerNodeCommunication {
        mempool_channel: ComponentCommunication::new(Some(tx_mempool), Some(rx_mempool)),
        batcher_channel: ComponentCommunication::new(Some(tx_batcher), Some(rx_batcher)),
        gateway_channel: ComponentCommunication::new(Some(tx_gateway), Some(rx_gateway)),
        mempool_p2p_propagator_channel: ComponentCommunication::new(
            Some(tx_mempool_p2p_propagator),
            Some(rx_mempool_p2p_propagator),
        ),
    }
}

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

pub fn create_component_client<ComponentClient>(
    execution_mode: ComponentExecutionMode,
    component_client: ComponentClient,
) -> Option<ComponentClient> {
    match execution_mode {
        ComponentExecutionMode::LocalExecutionWithRemoteDisabled
        | ComponentExecutionMode::LocalExecutionWithRemoteEnabled => Some(component_client),
        ComponentExecutionMode::Disabled => None,
    }
}

pub fn create_node_clients(
    config: &SequencerNodeConfig,
    channels: &mut SequencerNodeCommunication,
) -> SequencerNodeClients {
    let batcher_client: Option<SharedBatcherClient> = create_component_client(
        config.components.batcher.execution_mode.clone(),
        Arc::new(LocalBatcherClient::new(channels.take_batcher_tx())),
    );
    let mempool_client: Option<SharedMempoolClient> = create_component_client(
        config.components.mempool.execution_mode.clone(),
        Arc::new(LocalMempoolClient::new(channels.take_mempool_tx())),
    );
    let gateway_client: Option<SharedGatewayClient> = create_component_client(
        config.components.gateway.execution_mode.clone(),
        Arc::new(LocalGatewayClient::new(channels.take_gateway_tx())),
    );
    let mempool_p2p_propagator_client: Option<SharedMempoolP2pPropagatorClient> =
        create_component_client(
            config.components.mempool_p2p.execution_mode.clone(),
            Arc::new(LocalMempoolP2pPropagatorClient::new(
                channels.take_mempool_p2p_propagator_tx(),
            )),
        );
    SequencerNodeClients {
        batcher_client,
        mempool_client,
        gateway_client,
        mempool_p2p_propagator_client,
    }
}
