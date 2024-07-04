use starknet_mempool_infra::component_definitions::ComponentRequestAndResponseSender;
use starknet_mempool_types::communication::{
    MempoolClient, MempoolClientImpl, MempoolRequest, MempoolResponse,
};
use starknet_mempool_types::mempool_types::ThinTransaction;
use tokio::sync::mpsc::Sender;

#[derive(Clone)]
pub struct MockBatcher {
    mempool_client: MempoolClientImpl,
}

impl MockBatcher {
    pub fn new(
        mempool_sender: Sender<ComponentRequestAndResponseSender<MempoolRequest, MempoolResponse>>,
    ) -> Self {
        Self { mempool_client: MempoolClientImpl::new(mempool_sender) }
    }

    pub async fn get_txs(&self, n_txs: usize) -> Vec<ThinTransaction> {
        self.mempool_client.get_txs(n_txs).await.unwrap()
    }
}
