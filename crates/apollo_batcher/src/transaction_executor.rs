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
        block_timeout: Option<tokio::time::Instant>,
    ) -> Vec<TransactionExecutorResult<TransactionExecutionInfo>>;
    fn close_block(&mut self) -> TransactionExecutorResult<BlockExecutionSummary>;
}

impl<S: StateReader + Send + Sync> TransactionExecutorTrait for TransactionExecutor<S> {
    /// Adds the transactions to the generated block and returns the execution results.
    fn add_txs_to_block(
        &mut self,
        txs: &[BlockifierTransaction],
        block_timeout: Option<tokio::time::Instant>,
    ) -> Vec<TransactionExecutorResult<TransactionExecutionInfo>> {
        // Change deadline from tokio::time::Instant to std::time::Instant.
        let execution_deadine = block_timeout.map(|instant| instant.into());
        self.execute_txs(txs, execution_deadine)
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
