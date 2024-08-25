use std::sync::Arc;

use starknet_batcher_types::communication::{
    BatcherRequestAndResponseSender,
    LocalBatcherClientImpl,
    SharedBatcherClient,
};
use starknet_consensus_manager_types::communication::{
    ConsensusManagerRequestAndResponseSender,
    LocalConsensusManagerClientImpl,
    SharedConsensusManagerClient,
};
use starknet_mempool_infra::component_definitions::{
    ComponentCommunication,
    RemoteComponentCommunicationConfig,
};
use starknet_mempool_types::communication::{
    LocalMempoolClientImpl,
    MempoolClient,
    MempoolRequestAndResponseSender,
    RemoteMempoolClientImpl,
    SharedMempoolClient,
};
use tokio::sync::mpsc::{channel, Receiver, Sender};

use crate::config::{LocationType, MempoolNodeConfig};

pub struct MempoolNodeCommunication {
    batcher_channel: ComponentCommunication<BatcherRequestAndResponseSender>,
    consensus_manager_channel: ComponentCommunication<ConsensusManagerRequestAndResponseSender>,
    mempool_channel: ComponentCommunication<MempoolRequestAndResponseSender>,
}

impl MempoolNodeCommunication {
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
}

pub fn create_node_channels() -> MempoolNodeCommunication {
    const DEFAULT_INVOCATIONS_QUEUE_SIZE: usize = 32;
    let (tx_mempool, rx_mempool) =
        channel::<MempoolRequestAndResponseSender>(DEFAULT_INVOCATIONS_QUEUE_SIZE);

    let (tx_batcher, rx_batcher) =
        channel::<BatcherRequestAndResponseSender>(DEFAULT_INVOCATIONS_QUEUE_SIZE);

    let (tx_consensus_manager, rx_consensus_manager) =
        channel::<ConsensusManagerRequestAndResponseSender>(DEFAULT_INVOCATIONS_QUEUE_SIZE);

    MempoolNodeCommunication {
        mempool_channel: ComponentCommunication::new(Some(tx_mempool), Some(rx_mempool)),
        consensus_manager_channel: ComponentCommunication::new(
            Some(tx_consensus_manager),
            Some(rx_consensus_manager),
        ),
        batcher_channel: ComponentCommunication::new(Some(tx_batcher), Some(rx_batcher)),
    }
}

pub struct MempoolNodeClients {
    batcher_client: Option<SharedBatcherClient>,
    consensus_manager_client: Option<SharedConsensusManagerClient>,
    mempool_client: Option<SharedMempoolClient>,
    // TODO (Lev): Change to Option<Box<dyn MemPoolClient>>.
}

impl MempoolNodeClients {
    pub fn get_batcher_client(&self) -> Option<SharedBatcherClient> {
        self.batcher_client.clone()
    }

    pub fn get_consensus_manager_client(&self) -> Option<SharedConsensusManagerClient> {
        self.consensus_manager_client.clone()
    }

    pub fn get_mempool_client(&self) -> Option<SharedMempoolClient> {
        self.mempool_client.clone()
    }
}

pub fn create_node_clients(
    config: &MempoolNodeConfig,
    channels: &mut MempoolNodeCommunication,
) -> MempoolNodeClients {
    let batcher_client: Option<SharedBatcherClient> = if config.components.batcher.execute {
        Some(Arc::new(LocalBatcherClientImpl::new(channels.take_batcher_tx())))
    } else {
        None
    };
    let consensus_manager_client: Option<SharedConsensusManagerClient> =
        if config.components.consensus_manager.execute {
            Some(Arc::new(LocalConsensusManagerClientImpl::new(
                channels.take_consensus_manager_tx(),
            )))
        } else {
            None
        };
    let mempool_client: Option<SharedMempoolClient> = if config.components.gateway.execute {
        let mempool_client: Arc<dyn MempoolClient> = match config.components.mempool.location {
            LocationType::Local => {
                Arc::new(LocalMempoolClientImpl::new(channels.take_mempool_tx()))
            }
            LocationType::Remote => {
                let RemoteComponentCommunicationConfig { ip, port, retries } =
                    config.components.mempool.remote_config.clone().unwrap();

                Arc::new(RemoteMempoolClientImpl::new(ip, port, retries))
            }
        };
        Some(mempool_client)
    } else {
        None
    };
    MempoolNodeClients { batcher_client, consensus_manager_client, mempool_client }
}
