use async_trait::async_trait;
use tokio::sync::Mutex;

pub type AddTransactionCallType = u32;
pub type AddTransactionReturnType = usize;

#[async_trait]
pub trait MempoolTrait {
    async fn add_transaction(&self, tx: AddTransactionCallType) -> AddTransactionReturnType;
}

#[derive(Default)]
pub struct Mempool {
    transactions: Mutex<Vec<u32>>,
}

#[async_trait]
impl MempoolTrait for Mempool {
    async fn add_transaction(&self, tx: AddTransactionCallType) -> AddTransactionReturnType {
        let mut guarded_transactions = self.transactions.lock().await;
        guarded_transactions.push(tx);
        guarded_transactions.len()
    }
}
