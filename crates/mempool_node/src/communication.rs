use starknet_mempool_types::communication::MempoolRequestAndResponseSender;
use tokio::sync::mpsc::{channel, Receiver, Sender};

pub struct ComponentCommunication<T: Send + Sync> {
    tx: Sender<T>,
    rx: Option<Receiver<T>>,
}

impl<T: Send + Sync> ComponentCommunication<T> {
    fn get_tx(&self) -> Sender<T> {
        self.tx.clone()
    }
    fn get_rx(&mut self) -> Receiver<T> {
        self.rx.take().expect("Receiver already taken")
    }
}

pub struct MempoolNodeCommunication {
    mempool_channel: ComponentCommunication<MempoolRequestAndResponseSender>,
}

impl MempoolNodeCommunication {
    pub fn get_mempool_tx(&self) -> Sender<MempoolRequestAndResponseSender> {
        self.mempool_channel.get_tx()
    }
    pub fn get_mempool_rx(&mut self) -> Receiver<MempoolRequestAndResponseSender> {
        self.mempool_channel.get_rx()
    }
}

pub fn create_node_channels() -> MempoolNodeCommunication {
    const MEMPOOL_INVOCATIONS_QUEUE_SIZE: usize = 32;
    let (tx_mempool, rx_mempool) =
        channel::<MempoolRequestAndResponseSender>(MEMPOOL_INVOCATIONS_QUEUE_SIZE);
    MempoolNodeCommunication {
        mempool_channel: ComponentCommunication { tx: tx_mempool, rx: Some(rx_mempool) },
    }
}
