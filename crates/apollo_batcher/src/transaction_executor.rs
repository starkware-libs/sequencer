use blockifier::blockifier::concurrent_transaction_executor::ConcurrentTransactionExecutor;
use blockifier::blockifier::transaction_executor::{
    BlockExecutionSummary,
    TransactionExecutor,
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

impl<S: StateReader + Send + Sync + 'static> TransactionExecutorTrait for TransactionExecutor<S> {
    /// Adds the transactions to the generated block and returns the execution results.
    fn add_txs_to_block(
        &mut self,
        txs: &[BlockifierTransaction],
    ) -> Vec<TransactionExecutorResult<TransactionExecutionInfo>> {
        self.execute_txs(txs)
            .into_iter()
            .map(|res| res.map(|(tx_execution_info, _state_diff)| tx_execution_info))
            .collect()
    }
    /// Finalizes the block creation and returns the commitment state diff, visited
    /// segments mapping and bouncer.
    fn close_block(&mut self) -> TransactionExecutorResult<BlockExecutionSummary> {
        self.finalize()
    }
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
