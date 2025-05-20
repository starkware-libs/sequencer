use std::mem;

use blockifier::blockifier::transaction_executor::{
    BlockExecutionSummary,
    TransactionExecutor,
    TransactionExecutorResult,
};
use blockifier::concurrency::worker_logic::WorkerExecutor;
use blockifier::state::cached_state::CachedState;
use blockifier::state::state_api::{StateReader, UpdatableState};
use blockifier::state::stateful_compression::{allocate_aliases_in_storage, compress};
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

impl<S: UpdatableState + Send + Sync + 'static> TransactionExecutorTrait for WorkerExecutor<CachedState<S>> {
    fn add_txs_to_block(&mut self, txs: &[BlockifierTransaction]) -> Vec<TransactionExecutorResult<TransactionExecutionInfo>> {
        self.add_transactions_and_wait(txs)
            .into_iter()
            .map(|res| res.map(|(tx_execution_info, _state_diff)| tx_execution_info))
            .collect()
    }

    fn close_block(&mut self) -> TransactionExecutorResult<BlockExecutionSummary> {
        log::debug!(
            "Final block weights: {:?}.",
            self.bouncer.lock().expect("Bouncer lock failed.").get_accumulated_weights()
        );
        // TODO: Get n_committed_txs from the caller.
        let n_committed_txs = self.scheduler.get_n_committed_txs();
        let mut state_after_block =
            self.commit_chunk_and_recover_block_state(n_committed_txs);
        let alias_contract_address = self
            .block_context
            .versioned_constants()
            .os_constants
            .os_contract_addresses
            .alias_contract_address();
        if self.block_context.versioned_constants().enable_stateful_compression {
            allocate_aliases_in_storage(&mut state_after_block, alias_contract_address)?;
        }
        let state_diff = state_after_block.to_state_diff()?.state_maps;
        let compressed_state_diff =
            if self.block_context.versioned_constants().enable_stateful_compression {
                Some(compress(&state_diff, &mut state_after_block, alias_contract_address)?.into())
            } else {
                None
            };

        let mut bouncer = self.bouncer.lock().expect("Bouncer lock failed.");
        Ok(BlockExecutionSummary {
            state_diff: state_diff.into(),
            compressed_state_diff,
            bouncer_weights: *bouncer.get_accumulated_weights(),
            casm_hash_computation_data: mem::take(&mut bouncer.casm_hash_computation_data),
        })
    }
}
