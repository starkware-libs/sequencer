use blockifier::blockifier::transaction_executor::{
    TransactionExecutor,
    TransactionExecutorError,
    VisitedSegmentsMapping,
};
use blockifier::bouncer::BouncerWeights;
use blockifier::state::cached_state::CommitmentStateDiff;
use blockifier::state::state_api::StateReader;
use blockifier::transaction::transaction_execution::Transaction;
use starknet_api::state::StateDiff;
use thiserror::Error;

pub struct BlockifierRegressionTest<S: StateReader + Sync + Send> {
    pub executor: TransactionExecutor<S>,
    pub config: BlockifierRegressionTestConfig,
    pub expected_state_diff: StateDiff,
}

pub struct BlockifierRegressionTestConfig {
    pub to_dump: bool,
    pub dump_dir: Option<String>,
}

#[derive(Debug, Error)]
pub enum BlockifierRegressionTestError {
    #[error(transparent)]
    TransuctionExecutionError(#[from] TransactionExecutorError),
}

pub type BlockifierRegressionTestResult<T> = Result<T, BlockifierRegressionTestError>;

impl<S: StateReader + Sync + Send> BlockifierRegressionTest<S> {
    pub fn new(
        executor: TransactionExecutor<S>,
        config: BlockifierRegressionTestConfig,
        expected_state_diff: StateDiff,
    ) -> Self {
        Self { executor, config, expected_state_diff }
    }

    pub fn execute_txs(
        &mut self,
        txs: &[Transaction],
    ) -> BlockifierRegressionTestResult<(CommitmentStateDiff, VisitedSegmentsMapping, BouncerWeights)>
    {
        self.executor.execute_txs(txs);
        Ok(self.executor.finalize()?)
    }
}
