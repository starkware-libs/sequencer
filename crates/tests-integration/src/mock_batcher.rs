use starknet_api::executable_transaction::Transaction;
use starknet_mempool_types::communication::SharedMempoolClient;

pub struct MockBatcher {
    mempool_client: SharedMempoolClient,
}

impl MockBatcher {
    pub fn new(mempool_client: SharedMempoolClient) -> Self {
        Self { mempool_client }
    }

    pub async fn get_txs(&self, n_txs: usize) -> Vec<Transaction> {
        self.mempool_client.get_txs(n_txs).await.unwrap()
    }
}
