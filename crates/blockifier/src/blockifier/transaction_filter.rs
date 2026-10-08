use std::sync::Arc;

use crate::transaction::objects::{TransactionExecutionInfo, TransactionExecutionResult};
use crate::transaction::transaction_execution::Transaction;

/// A check on a successfully executed transaction, applied before its writes reach the block
/// state. A transaction that fails the check is rejected, as if its execution had failed.
pub trait TransactionFilter: Send + Sync {
    fn check(
        &self,
        tx: &Transaction,
        tx_execution_info: &TransactionExecutionInfo,
    ) -> TransactionExecutionResult<()>;
}

pub type SharedTransactionFilter = Arc<dyn TransactionFilter>;
