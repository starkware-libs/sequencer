use blockifier::blockifier::concurrent_transaction_executor::ConcurrentTransactionExecutor;
use blockifier::blockifier::transaction_executor::{
    BlockExecutionSummary,
    TransactionExecutionOutput,
    TransactionExecutorResult,
};
use blockifier::state::state_api::StateReader;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
pub trait TransactionExecutorTrait: Send {
    fn add_txs_to_block(
        &mut self,
        txs: &[BlockifierTransaction],
    ) -> Vec<TransactionExecutorResult<TransactionExecutionOutput>>;

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

impl<S: StateReader + Send + Sync + 'static> TransactionExecutorTrait
    for ConcurrentTransactionExecutor<S>
{
    /// Adds the transactions to the generated block and returns the execution results.
    fn add_txs_to_block(
        &mut self,
        txs: &[BlockifierTransaction],
    ) -> Vec<TransactionExecutorResult<TransactionExecutionOutput>> {
        self.add_txs_and_wait(txs)
    }

    /// Finalizes the block creation and returns the commitment state diff, visited
    /// segments mapping and bouncer.
    ///
    /// Every block must be closed with either `close_block` or `abort_block`.
    #[allow(clippy::result_large_err)]
    fn close_block(&mut self) -> TransactionExecutorResult<BlockExecutionSummary> {
        ConcurrentTransactionExecutor::close_block(self)
    }

    /// Notifies the transaction executor that the block is aborted.
    /// This allows the worker threads to continue to the next block.
    ///
    /// Every block must be closed with either `close_block` or `abort_block`.
    fn abort_block(&mut self) {
        ConcurrentTransactionExecutor::abort_block(self)
    }
}
