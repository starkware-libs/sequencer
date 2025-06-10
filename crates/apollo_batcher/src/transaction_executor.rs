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
    /// Starts executing the given transactions.
    fn add_txs_to_block(&mut self, txs: &[BlockifierTransaction]);

    /// Returns the new execution results of the transactions that were executed so far, starting
    /// from the last call to `get_new_results`.
    fn get_new_results(&mut self) -> Vec<TransactionExecutorResult<TransactionExecutionInfo>>;

    /// Returns true if the block is full or the deadline is reached.
    fn is_done(&self) -> bool;

    /// Finalizes the block creation and returns the commitment state diff, visited
    /// segments mapping and bouncer.
    ///
    /// Every block must be closed with either `close_block` or `abort_block`.
    #[allow(clippy::result_large_err)]
    fn close_block(&mut self) -> TransactionExecutorResult<BlockExecutionSummary>;

    /// Notifies the transaction executor that the block is aborted.
    /// This allows the worker threads to continue to the next block.
    ///
    /// Every block must be closed with either `close_block` or `abort_block`.
    fn abort_block(&mut self);
}

/// See [TransactionExecutorTrait] for documentation.
impl<S: StateReader + Send + Sync + 'static> TransactionExecutorTrait
    for ConcurrentTransactionExecutor<S>
{
    fn add_txs_to_block(&mut self, txs: &[BlockifierTransaction]) {
        self.add_txs(txs);
    }

    fn get_new_results(&mut self) -> Vec<TransactionExecutorResult<TransactionExecutionInfo>> {
        ConcurrentTransactionExecutor::get_new_results(self)
            .into_iter()
            .map(|res| res.map(|(tx_execution_info, _state_diff)| tx_execution_info))
            .collect()
    }

    fn is_done(&self) -> bool {
        ConcurrentTransactionExecutor::is_done(self)
    }

    #[allow(clippy::result_large_err)]
    fn close_block(&mut self) -> TransactionExecutorResult<BlockExecutionSummary> {
        ConcurrentTransactionExecutor::close_block(self)
    }

    fn abort_block(&mut self) {
        ConcurrentTransactionExecutor::abort_block(self)
    }
}
