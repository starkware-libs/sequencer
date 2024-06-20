use std::sync::Arc;

use starknet_mempool_types::communication::{
    MempoolClientImpl, MempoolRequestAndResponseSender, SharedMempoolClient,
};
use tokio::sync::mpsc::{channel, Receiver, Sender};

use crate::config::MempoolNodeConfig;

pub struct ComponentCommunication<T: Send + Sync> {
    tx: Option<Sender<T>>,
    rx: Option<Receiver<T>>,
}

impl<T: Send + Sync> ComponentCommunication<T> {
    fn take_tx(&self) -> Sender<T> {
        self.tx.to_owned().expect("Sender should be available, could be taken only once")
    }
    fn take_rx(&mut self) -> Receiver<T> {
        self.rx.take().expect("Receiver should be available, could be taken only once")
    }
}

pub struct MempoolNodeCommunication {
    mempool_channel: ComponentCommunication<MempoolRequestAndResponseSender>,
}

impl MempoolNodeCommunication {
    pub fn take_mempool_tx(&self) -> Sender<MempoolRequestAndResponseSender> {
        self.mempool_channel.take_tx()
    }
    pub fn take_mempool_rx(&mut self) -> Receiver<MempoolRequestAndResponseSender> {
        self.mempool_channel.take_rx()
    }
}

pub fn create_node_channels() -> MempoolNodeCommunication {
    const MEMPOOL_INVOCATIONS_QUEUE_SIZE: usize = 32;
    let (tx_mempool, rx_mempool) =
        channel::<MempoolRequestAndResponseSender>(MEMPOOL_INVOCATIONS_QUEUE_SIZE);
    MempoolNodeCommunication {
        mempool_channel: ComponentCommunication { tx: Some(tx_mempool), rx: Some(rx_mempool) },
    }
}

pub struct MempoolNodeClients {
    mempool_client: Option<SharedMempoolClient>,
    // TODO (Lev 25/06/2024): Change to Option<Box<dyn MemPoolClient>>.
}

impl MempoolNodeClients {
    pub fn get_mempool_client(&self) -> Option<SharedMempoolClient> {
        self.mempool_client.clone()
    }
}

pub fn create_node_clients(
    config: &MempoolNodeConfig,
    channels: &MempoolNodeCommunication,
) -> MempoolNodeClients {
    let mempool_client: Option<SharedMempoolClient> =
        match config.components.gateway_component.execute {
            true => Some(Arc::new(MempoolClientImpl::new(channels.take_mempool_tx()))),
            false => None,
        };
    MempoolNodeClients { mempool_client }
}
