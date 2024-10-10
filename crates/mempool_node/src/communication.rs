use std::sync::Arc;

use starknet_batcher_types::communication::{
    BatcherRequestAndResponseSender,
    LocalBatcherClient,
    SharedBatcherClient,
};
use starknet_consensus_manager_types::communication::{
    ConsensusManagerRequestAndResponseSender,
    LocalConsensusManagerClient,
    SharedConsensusManagerClient,
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

use crate::config::SequencerNodeConfig;

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

pub fn create_node_clients(
    config: &SequencerNodeConfig,
    channels: &mut SequencerNodeCommunication,
) -> SequencerNodeClients {
    let batcher_client: Option<SharedBatcherClient> = match config.components.batcher.execute {
        true => Some(Arc::new(LocalBatcherClient::new(channels.take_batcher_tx()))),
        false => None,
    };
    let consensus_manager_client: Option<SharedConsensusManagerClient> =
        match config.components.consensus_manager.execute {
            true => Some(Arc::new(LocalConsensusManagerClient::new(
                channels.take_consensus_manager_tx(),
            ))),
            false => None,
        };
    let mempool_client: Option<SharedMempoolClient> = match config.components.mempool.execute {
        true => Some(Arc::new(LocalMempoolClient::new(channels.take_mempool_tx()))),
        false => None,
    };
    let gateway_client: Option<SharedGatewayClient> = match config.components.gateway.execute {
        true => Some(Arc::new(LocalGatewayClient::new(channels.take_gateway_tx()))),
        false => None,
    };
    SequencerNodeClients {
        batcher_client,
        consensus_manager_client,
        mempool_client,
        gateway_client,
    }
}
