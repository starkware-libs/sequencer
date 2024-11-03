use blockifier::blockifier::transaction_executor::{
    TransactionExecutor,
    TransactionExecutorResult,
    VisitedSegmentsMapping,
};
use blockifier::bouncer::BouncerWeights;
use blockifier::state::cached_state::CommitmentStateDiff;
use blockifier::state::state_api::StateReader;
use blockifier::transaction::account_transaction::AccountTransaction;
use blockifier::transaction::objects::TransactionExecutionInfo;
use blockifier::transaction::transaction_execution::Transaction as BlockifierTransaction;
#[cfg(test)]
use mockall::automock;

use crate::block_builder::BlockBuilderResult;
use crate::transaction_dispatcher::TransactionEvent;

#[cfg_attr(test, automock)]
pub trait TransactionExecutorTrait: Send {
    fn add_txs_to_block(
        &mut self,
        txs: &[BlockifierTransaction],
    ) -> Vec<TransactionExecutorResult<TransactionExecutionInfo>>;
    fn close_block(
        &mut self,
    ) -> TransactionExecutorResult<(CommitmentStateDiff, VisitedSegmentsMapping, BouncerWeights)>;
}

impl<S: StateReader + Send + Sync> TransactionExecutorTrait for TransactionExecutor<S> {
    /// Adds the transactions to the generated block and returns the execution results.
    fn add_txs_to_block(
        &mut self,
        txs: &[BlockifierTransaction],
    ) -> Vec<TransactionExecutorResult<TransactionExecutionInfo>> {
        self.execute_txs(txs)
    }
    /// Finalizes the block creation and returns the commitment state diff, visited
    /// segments mapping and bouncer.
    fn close_block(
        &mut self,
    ) -> TransactionExecutorResult<(CommitmentStateDiff, VisitedSegmentsMapping, BouncerWeights)>
    {
        self.finalize()
    }
}

pub fn into_executor_tx_chunk(
    tx_events_chunk: &Vec<TransactionEvent>,
) -> BlockBuilderResult<Vec<BlockifierTransaction>> {
    let mut executor_input_chunk = vec![];
    for tx_event in tx_events_chunk {
        match tx_event {
            TransactionEvent::Transaction(tx) => {
                executor_input_chunk
                    .push(BlockifierTransaction::Account(AccountTransaction::try_from(tx)?));
            }
            TransactionEvent::Finish => {}
        }
    }
    Ok(executor_input_chunk)
}
