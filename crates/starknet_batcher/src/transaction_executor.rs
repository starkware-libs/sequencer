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

impl<S: StateReader + Send + Sync> TransactionExecutorTrait for TransactionExecutor<S> {
    /// Adds the transactions to the generated block and returns the execution results.
    fn add_txs_to_block(
        &mut self,
        txs: &[BlockifierTransaction],
    ) -> Vec<TransactionExecutorResult<TransactionExecutionInfo>> {
        println!("Inside add_txs_to_block, calling execute_txs");
        let x = self.execute_txs(txs);
        println!("Inside add_txs_to_block, executed txs, mapping to tx_execution_info");
        let y = x.into_iter()
            .map(|res| res.map(|(tx_execution_info, _state_diff)| tx_execution_info))
            .collect();
        println!("Inside add_txs_to_block, returning tx_execution_info");
        y
    }
    /// Finalizes the block creation and returns the commitment state diff, visited
    /// segments mapping and bouncer.
    fn close_block(&mut self) -> TransactionExecutorResult<BlockExecutionSummary> {
        self.finalize()
    }
}
