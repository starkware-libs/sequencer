use std::sync::Arc;

use crate::transaction_pool::TransactionPool;
use crate::utils::Clock;
pub struct TransactionManager {
    pub tx_pool: TransactionPool,
}

impl TransactionManager {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        TransactionManager { tx_pool: TransactionPool::new(clock) }
    }
}
