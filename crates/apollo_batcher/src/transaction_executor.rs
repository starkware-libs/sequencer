use blockifier::blockifier::concurrent_transaction_executor::ConcurrentTransactionExecutor;
use blockifier::blockifier::transaction_executor::{
    BlockExecutionSummary,
    TransactionExecutorResult,
};
use blockifier::state::state_api::StateReader;
use blockifier::transaction::objects::TransactionExecutionInfo;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
pub trait TransactionExecutorTrait: Send {
    fn add_txs_to_block(
        &mut self,
        txs: &[BlockifierTransaction],
    ) -> Vec<TransactionExecutorResult<TransactionExecutionInfo>>;
    fn close_block(&mut self) -> TransactionExecutorResult<BlockExecutionSummary>;
}

impl<S: StateReader + Send + Sync + 'static> TransactionExecutorTrait
    for ConcurrentTransactionExecutor<S>
{
    fn add_txs_to_block(
        &mut self,
        txs: &[BlockifierTransaction],
    ) -> Vec<TransactionExecutorResult<TransactionExecutionInfo>> {
        self.add_transactions_and_wait(txs)
            .into_iter()
            .map(|res| res.map(|(tx_execution_info, _state_diff)| tx_execution_info))
            .collect()
    }

    fn close_block(&mut self) -> TransactionExecutorResult<BlockExecutionSummary> {
        ConcurrentTransactionExecutor::close_block(self)
    }
}
