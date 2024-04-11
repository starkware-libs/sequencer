use async_trait::async_trait;

pub type AddTransactionCallType = u32;
pub type AddTransactionReturnType = usize;

#[async_trait]
pub trait MempoolTrait {
    async fn add_transaction(&mut self, tx: AddTransactionCallType) -> AddTransactionReturnType;
}

#[derive(Default)]
pub struct Mempool {
    transactions: Vec<u32>,
}

impl Mempool {
    pub fn new() -> Self {
        Self {
            transactions: vec![],
        }
    }
}

#[async_trait]
impl MempoolTrait for Mempool {
    async fn add_transaction(&mut self, tx: AddTransactionCallType) -> AddTransactionReturnType {
        self.transactions.push(tx);
        self.transactions.len()
    }
}
