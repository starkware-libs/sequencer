use starknet_mempool_types::communication::SharedMempoolClient;
use starknet_mempool_types::mempool_types::ThinTransaction;

pub struct MockBatcher {
    mempool_client: SharedMempoolClient,
}

impl MockBatcher {
    pub fn new(mempool_client: SharedMempoolClient) -> Self {
        Self { mempool_client }
    }

    pub async fn get_txs(&self, n_txs: usize) -> Vec<ThinTransaction> {
        self.mempool_client.get_txs(n_txs).await.unwrap()
    }
}
